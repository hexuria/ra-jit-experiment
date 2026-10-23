//! What `#[ra_jit::cache]` keeps the results of a function by: a hash of the code of its body
//! and of everything that code can reach, as ra-jit compiled it. The attribute keeps the body
//! in a closure and has the function ask `ra_jit::__private::__ra_jit_cache_code` for the code
//! of that closure, which rustc answers with nothing, and so caches nothing. ra-jit compiles
//! the call into the answer: the hash, or why the function can't be cached.
//!
//! The hash is of the compiled code without what tells where it was written, so comments,
//! formatting and lines moved keep it, and an edit to anything the body calls, directly or
//! through others, changes it. Each function reached is hashed as the instance compiled, so
//! the instances of a generic function have hashes of their own. A call through a `dyn` goes
//! to a vtable, and the code reached builds every vtable it can call through: the methods of
//! those vtables are reached as the rest is. The arguments can't be a `dyn`.
//!
//! A function can be cached when what it gives depends on its arguments alone. It can't when
//! what it reaches reads a static that can change, a thread-local or a foreign static, calls a
//! foreign function other than the allocator, `mem*` and libm, or the code of the workspace
//! dereferences a raw pointer, takes an address as a number or has assembly in it, or core's
//! formats, hashes or orders pointers by their addresses or gives one as a number. Neither can
//! it when an argument can change behind a shared reference, as a `Cell` or an atomic can, or
//! holds a raw pointer of a type of the workspace. The standard library's I/O, time,
//! randomness and environment are all foreign calls or statics that change, so they are seen.

use std::hash::{Hash, Hasher};

use base_db::{Crate, LangCrateOrigin};
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::ModuleRelocTarget;
use hir_def::{
    AdtId, DefWithBodyId, FunctionId, HasModule, ItemContainerId, Lookup, StaticId, VariantId,
    attrs::AttrFlags,
    signatures::{FunctionSignature, StaticFlags, StaticSignature, TraitSignature},
};
use rustc_hash::{FxHashMap, FxHashSet};
use rustc_type_ir::inherent::{GenericArgs as _, IntoKind};
use salsa::plumbing::{AsId, FromId};

use hir_def::CallableDefId;
use hir_ty::{
    InferBodyId,
    db::{HirDatabase, InternedClosureId, InternedCoroutineId},
    mir::{
        CastKind, MirBody, MirSpan, Operand, OperandKind, ProjectionElem, Rvalue, StatementKind,
        TerminatorKind,
    },
    next_solver::{DbInterner, GenericArgs, Ty, TyKind},
    traits::StoredParamEnvAndCrate,
};

use super::{
    CodegenError, CompiledFunction, ConstBlob, DataRequest, FuncTarget, FunctionCx, Instance,
    InstanceDef, MEM, Requests, Result, compiled_instance, compiled_root, compiled_static,
    fragment::{Code, TextPart},
    home::Homes,
    is_thread_local, link_name, ptr, type_id,
};

/// The function a cached function asks for the code of its body.
const CODE: &str = "__ra_jit_cache_code";
/// The function whose instance names how the arguments and the result are written: reached
/// for the hash, though what it does is not the cached function's.
const CODEC: &str = "__ra_jit_codec";
/// Changes when what is hashed does, so no hash of before means anything now.
const FORMAT: u32 = 1;

/// Whether `func` is the function a cached function asks for the code of its body.
pub(super) fn is_code(db: &dyn HirDatabase, func: FunctionId) -> bool {
    FunctionSignature::of(db, func).name.as_str() == CODE
}

fn is_codec(db: &dyn HirDatabase, func: FunctionId) -> bool {
    FunctionSignature::of(db, func).name.as_str() == CODEC
}

/// The closure that `#[ra_jit::cache]` put the body of `func` in, when it is a cached
/// function: the one it asks for the code of.
pub(super) fn cached_closure(db: &dyn HirDatabase, func: FunctionId) -> Option<salsa::Id> {
    if !uses_cache(db, func.krate(db)) {
        return None;
    }
    cached_closure_of(db, func).map(|it| it.as_id())
}

/// Whether `krate` can have cached functions: the attribute calls into the `ra_jit` crate, so
/// a crate without it as a dependency has none, and costs nothing to look through.
pub(super) fn uses_cache(db: &dyn HirDatabase, krate: Crate) -> bool {
    krate.data(db).dependencies.iter().any(|dep| {
        dep.crate_id
            .extra_data(db)
            .display_name
            .as_ref()
            .is_some_and(|name| name.crate_name().symbol().as_str() == "ra_jit")
    })
}

#[salsa::tracked]
fn cached_closure_of<'db>(
    db: &'db dyn HirDatabase,
    func: FunctionId,
) -> Option<InternedClosureId<'db>> {
    let body = db.mir_body(func.into()).ok()?;
    body.basic_blocks.iter().find_map(|(_, block)| {
        let TerminatorKind::Call { func: callee, .. } = &block.terminator.as_ref()?.kind else {
            return None;
        };
        let (callee, args) = fn_item(callee)?;
        if !is_code(db, callee) {
            return None;
        }
        let TyKind::Closure(id, _) = args.type_at(0).kind() else { return None };
        Some(id.0)
    })
}

/// The function a constant operand names, with its generic arguments.
fn fn_item<'db>(operand: &Operand) -> Option<(FunctionId, GenericArgs<'db>)> {
    let ty: Ty<'db> = match &operand.kind {
        OperandKind::Constant { ty, .. } => ty.as_ref(),
        OperandKind::Allocation { allocation } => allocation.as_ref().ty,
        _ => return None,
    };
    let TyKind::FnDef(def, args) = ty.kind() else { return None };
    match def.0 {
        CallableDefId::FunctionId(func) => Some((func, args)),
        _ => None,
    }
}

/// The hash of the code of the closure a cached function keeps its body in and of all it
/// reaches, or why the function can't be cached.
#[salsa::tracked(returns(ref))]
pub(super) fn cache_code(
    db: &dyn HirDatabase,
    closure: Instance,
    env: StoredParamEnvAndCrate,
) -> std::result::Result<u128, String> {
    let mut walk = Walk::new(db);
    let root = walk.reach(Node::Instance(closure.clone(), env.clone()));
    let mut next = 0;
    while let Some(node) = walk.nodes.get(next).map(|it| it.node.clone()) {
        walk.hash_node(next as u32, &node);
        next += 1;
    }
    let name = cached_name(db, &closure);
    if let Some(why) = walk.impure(root) {
        return Err(format!("`{name}` is not cached: {why}"));
    }
    if let Some(why) = arguments_trouble(db, &closure, &env) {
        return Err(format!("`{name}` is not cached: {why}"));
    }
    let mut hasher = rustc_stable_hash::StableSipHasher128::new();
    (FORMAT, super::target::Target::current().triple(), super::store::toolchain())
        .hash(&mut hasher);
    closure.checked.hash(&mut hasher);
    for arg in closure.args.as_ref().iter() {
        arg.ty().and_then(|ty| hir_ty::mir::type_name::rustc_type_name(db, ty)).hash(&mut hasher);
    }
    for segment in &walk.segments {
        segment.hash(&mut hasher);
    }
    let type_id::Fingerprint(low, high) = hasher.finish();
    Ok(u128::from(low) | u128::from(high) << 64)
}

/// The name of the function whose body `closure` is, as a backtrace has it.
pub(super) fn cached_name(db: &dyn HirDatabase, closure: &Instance) -> String {
    let InstanceDef::Closure(id) = closure.def else { return "?".to_owned() };
    match InternedClosureId::from_id(id).loc(db).owner {
        InferBodyId::DefWithBodyId(DefWithBodyId::FunctionId(func)) => {
            function_name(db, func, closure.args.as_ref())
        }
        _ => "?".to_owned(),
    }
}

fn function_name(db: &dyn HirDatabase, func: FunctionId, args: GenericArgs<'_>) -> String {
    hir_ty::mir::type_name::rustc_function_name(db, func, args.as_slice())
        .unwrap_or_else(|| FunctionSignature::of(db, func).name.as_str().to_owned())
}

fn crate_name(db: &dyn HirDatabase, krate: Crate) -> String {
    krate.extra_data(db).display_name.as_ref().map_or_else(|| "?".to_owned(), |it| it.to_string())
}

/// What the hash goes through: the functions and statics compiled, and what has no code of
/// ours, by its name.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Node {
    Root(FunctionId),
    Instance(Instance, StoredParamEnvAndCrate),
    Static(StaticId),
    /// A function or static known by its name alone, and what keeps it from being pure.
    Leaf(String, Option<String>),
}

/// What a node refers to the next by, to find where that is written.
#[derive(Clone, Copy)]
enum Site {
    Func(FunctionId),
    Static(StaticId),
    None,
}

struct Edge {
    to: u32,
    site: Site,
    /// The edge is to how the arguments and result are written, which runs apart from the
    /// body, so what it does keeps nothing from being cached.
    codec: bool,
}

struct Seen {
    node: Node,
    edges: Vec<Edge>,
    why: Option<Why>,
}

/// What keeps a node from being pure.
struct Why {
    /// What the node does, as said of it: "dereferences a raw pointer". Of a foreign function
    /// or a static it is said of what uses it: "calls the foreign function `write`".
    text: String,
    of_user: bool,
    /// Where that is written, when that is told.
    place: Option<String>,
}

impl Why {
    fn own(text: impl Into<String>) -> Option<Why> {
        Some(Why { text: text.into(), of_user: false, place: None })
    }

    fn of_user(text: impl Into<String>) -> Option<Why> {
        Some(Why { text: text.into(), of_user: true, place: None })
    }
}

struct Walk<'db> {
    db: &'db dyn HirDatabase,
    homes: Homes,
    nodes: Vec<Seen>,
    index: FxHashMap<Node, u32>,
    /// What each node is when hashed, in the order the nodes were found.
    segments: Vec<u128>,
}

impl<'db> Walk<'db> {
    fn new(db: &'db dyn HirDatabase) -> Walk<'db> {
        Walk {
            db,
            homes: Homes::default(),
            nodes: Vec::new(),
            index: FxHashMap::default(),
            segments: Vec::new(),
        }
    }

    fn reach(&mut self, node: Node) -> u32 {
        if let Some(&at) = self.index.get(&node) {
            return at;
        }
        let at = self.nodes.len() as u32;
        let why = match &node {
            Node::Leaf(_, Some(why)) => Why::of_user(why.clone()),
            _ => None,
        };
        self.nodes.push(Seen { node: node.clone(), edges: Vec::new(), why });
        self.index.insert(node, at);
        at
    }

    fn edge(&mut self, from: u32, node: Node, site: Site, codec: bool) -> u32 {
        let to = self.reach(node);
        self.nodes[from as usize].edges.push(Edge { to, site, codec });
        to
    }

    /// What a function an instance names is in the walk.
    fn function(
        &mut self,
        instance: &Instance,
        env: &StoredParamEnvAndCrate,
    ) -> (Node, Site, bool) {
        let db = self.db;
        let InstanceDef::Fn(func) = instance.def else {
            let site = match instance.def {
                InstanceDef::VtableShim(func) => Site::Func(func),
                _ => Site::None,
            };
            return (Node::Instance(instance.clone(), env.clone()), site, false);
        };
        let site = Site::Func(func);
        if let Some(closure) = cached_closure(db, func) {
            // Another cached function, or this one called again: its body is what it runs
            // when its result is not kept, and what it reaches from there is the body's.
            let closure = Instance { def: InstanceDef::Closure(closure), ..instance.clone() };
            let env = self.homes.home(db, &closure, env);
            return (Node::Instance(closure, env), site, false);
        }
        let name = || function_name(db, func, instance.args.as_ref());
        if let Some(symbol) = link_name(db, func) {
            let why =
                (!pure_foreign(&symbol)).then(|| format!("calls the foreign function `{symbol}`"));
            return (Node::Leaf(symbol, why), site, false);
        }
        if diverges(db, func) {
            // A panic: what it prints is no result.
            return (Node::Leaf(name(), None), site, false);
        }
        if let Some(what) = gives_address(db, func) {
            let name = name();
            let why = format!("calls `{name}`, which {what}");
            return (Node::Leaf(name, Some(why)), site, false);
        }
        let codec = is_codec(db, func);
        let named = instance.args.as_ref().iter().all(|it| it.region().is_some());
        if !named {
            return (Node::Instance(instance.clone(), env.clone()), site, codec);
        }
        match func.krate(db).data(db).origin {
            // Neither does I/O, nor has a static that changes but an atomic of its own, which
            // only what is given one reaches.
            base_db::CrateOrigin::Lang(LangCrateOrigin::Core | LangCrateOrigin::Alloc) => {
                (Node::Leaf(name(), None), site, codec)
            }
            _ => (Node::Root(func), site, codec),
        }
    }

    fn hash_node(&mut self, at: u32, node: &Node) {
        let db = self.db;
        if std::env::var_os("RA_JIT_FN_CACHE_TRACE").is_some() {
            eprintln!("fn-cache: {at} {}", describe(db, node));
        }
        let mut hasher = rustc_stable_hash::StableSipHasher128::new();
        match node {
            Node::Root(func) => {
                0u8.hash(&mut hasher);
                match compiled_root(db, *func) {
                    Ok(compiled) => {
                        self.function_code(at, &mut hasher, compiled);
                        self.look_into(at, InferBodyId::DefWithBodyId((*func).into()), None);
                    }
                    Err(e) => self.cannot(at, &mut hasher, e),
                }
            }
            Node::Instance(instance, env) => {
                1u8.hash(&mut hasher);
                match compiled_instance(db, instance.clone(), env.clone()) {
                    Ok(compiled) => {
                        self.function_code(at, &mut hasher, compiled);
                        match instance.def {
                            InstanceDef::Fn(func) => {
                                self.look_into(at, InferBodyId::DefWithBodyId(func.into()), None)
                            }
                            InstanceDef::Closure(id) => {
                                let closure = InternedClosureId::from_id(id);
                                self.look_into(at, closure.loc(db).owner, Some(closure));
                            }
                            _ => {}
                        }
                    }
                    Err(e) => self.cannot(at, &mut hasher, e),
                }
            }
            Node::Static(id) => {
                2u8.hash(&mut hasher);
                match compiled_static(db, *id) {
                    Ok(compiled) => {
                        if compiled.writable {
                            let why = format!("reads {}, which can change", describe(db, node));
                            self.nodes[at as usize].why = Why::of_user(why);
                        }
                        (&compiled.blob, compiled.writable).hash(&mut hasher);
                        let own = self.is_own(at);
                        self.requests(at, &mut hasher, &compiled.requests, own);
                    }
                    Err(e) => self.cannot(at, &mut hasher, e),
                }
            }
            Node::Leaf(name, _) => (3u8, name).hash(&mut hasher),
        }
        let type_id::Fingerprint(low, high) = hasher.finish();
        self.segments.push(u128::from(low) | u128::from(high) << 64);
    }

    /// Notes that the node did not compile: nothing tells what it does then.
    fn cannot(&mut self, at: u32, hasher: &mut impl Hasher, e: &CodegenError) {
        self.nodes[at as usize].why = Why::own(format!("can't be compiled by ra-jit ({e})"));
        e.to_string().hash(hasher);
    }

    fn function_code(&mut self, at: u32, hasher: &mut impl Hasher, compiled: &CompiledFunction) {
        hash_code(hasher, &compiled.code);
        let own = self.is_own(at);
        self.requests(at, hasher, &compiled.requests, own);
    }

    /// Whether the node is code of a crate of the workspace, which is held to more than the
    /// rest: it is the code the user is told to look at.
    fn is_own(&self, at: u32) -> bool {
        let db = self.db;
        let krate = match &self.nodes[at as usize].node {
            Node::Root(func) => func.krate(db),
            Node::Instance(instance, _) => match instance.def {
                InstanceDef::Fn(func) | InstanceDef::VtableShim(func) => func.krate(db),
                InstanceDef::Closure(id) => {
                    InternedClosureId::from_id(id).loc(db).owner.module(db).krate(db)
                }
                InstanceDef::Coroutine(id) | InstanceDef::CoroutineByMove(id) => {
                    InternedCoroutineId::from_id(id).loc(db).owner.module(db).krate(db)
                }
                _ => return false,
            },
            Node::Static(id) => id.krate(db),
            Node::Leaf(..) => return false,
        };
        krate.data(db).origin.is_local()
    }

    fn requests(&mut self, at: u32, hasher: &mut impl Hasher, requests: &Requests, own: bool) {
        let db = self.db;
        requests.funcs.len().hash(hasher);
        for request in &requests.funcs {
            request.sig.hash(hasher);
            match &request.target {
                FuncTarget::Symbol(symbol) => (0u8, symbol).hash(hasher),
                FuncTarget::Path(path) => (1u8, path).hash(hasher),
                FuncTarget::Asm(text) => {
                    (2u8, text).hash(hasher);
                    if own && self.nodes[at as usize].why.is_none() {
                        self.nodes[at as usize].why = Why::own("has assembly in it");
                    }
                }
                FuncTarget::Instance(instance, env) => {
                    let (node, site, codec) = self.function(instance, env);
                    (3u8, self.edge(at, node, site, codec)).hash(hasher);
                }
            }
        }
        requests.datas.len().hash(hasher);
        for request in &requests.datas {
            match request {
                DataRequest::Blob { blob, identity } => (0u8, blob, identity).hash(hasher),
                DataRequest::Vtable { ty, trait_ref, blob } => {
                    1u8.hash(hasher);
                    hir_ty::mir::type_name::rustc_type_name(db, ty.as_ref()).hash(hasher);
                    trait_ref
                        .as_ref()
                        .map(|(id, args)| {
                            let name = TraitSignature::of(db, *id).name.as_str().to_owned();
                            let args: Vec<_> = args
                                .as_ref()
                                .iter()
                                .map(|it| {
                                    it.ty().and_then(|ty| {
                                        hir_ty::mir::type_name::rustc_type_name(db, ty)
                                    })
                                })
                                .collect();
                            (crate_name(db, id.krate(db)), name, args)
                        })
                        .hash(hasher);
                    blob.hash(hasher);
                }
                DataRequest::Static(id) => {
                    let node = self.static_node(*id);
                    (2u8, self.edge(at, node, Site::Static(*id), false)).hash(hasher);
                }
                // Where code is written, which the hash leaves out.
                DataRequest::Location(_) => 3u8.hash(hasher),
                DataRequest::Declaration(_) => 4u8.hash(hasher),
                DataRequest::FilePath(_) => 5u8.hash(hasher),
                DataRequest::Text(parts) => {
                    6u8.hash(hasher);
                    for part in parts {
                        match part {
                            TextPart::Text(text) => (0u8, text).hash(hasher),
                            TextPart::FilePath(_) => 1u8.hash(hasher),
                        }
                    }
                }
            }
        }
    }

    fn static_node(&mut self, id: StaticId) -> Node {
        let db = self.db;
        let signature = StaticSignature::of(db, id);
        if signature.flags.contains(StaticFlags::EXTERN) {
            let name = AttrFlags::link_name(db, id.into())
                .map_or_else(|| signature.name.as_str().to_owned(), |it| it.as_str().to_owned());
            let why = (!name.starts_with("__rust_"))
                .then(|| format!("reads the foreign static `{name}`"));
            return Node::Leaf(name, why);
        }
        if is_thread_local(db, id) {
            let name = static_name(db, id);
            return Node::Leaf(name.clone(), Some(format!("uses the thread-local `{name}`")));
        }
        Node::Static(id)
    }

    /// Looks through the MIR of code of the workspace for what its machine code does not
    /// tell: a raw pointer dereferenced, or an address taken as a number.
    fn look_into(
        &mut self,
        at: u32,
        owner: InferBodyId<'db>,
        closure: Option<InternedClosureId<'db>>,
    ) {
        if !self.is_own(at) || self.nodes[at as usize].why.is_some() {
            return;
        }
        let db = self.db;
        let body = match closure {
            Some(closure) => db.mir_body_for_closure(closure),
            None => db.mir_body(owner),
        };
        let Ok(body) = body else { return };
        if let Some((why, span)) = raw_use(body) {
            let place = location(db, body, span);
            self.nodes[at as usize].why = Some(Why { text: why.to_owned(), of_user: false, place });
        }
    }

    /// Why what `from` reaches is not pure, by the shortest way to the first that is not:
    /// the edges to the codec are left out.
    fn impure(&self, from: u32) -> Option<String> {
        let mut parent: FxHashMap<u32, (u32, usize)> = FxHashMap::default();
        let mut seen = FxHashSet::default();
        let mut queue = std::collections::VecDeque::from([from]);
        seen.insert(from);
        while let Some(at) = queue.pop_front() {
            if let Some(why) = &self.nodes[at as usize].why {
                return Some(self.told(at, &parent, why));
            }
            for (i, edge) in self.nodes[at as usize].edges.iter().enumerate() {
                if !edge.codec && seen.insert(edge.to) {
                    parent.insert(edge.to, (at, i));
                    queue.push_back(edge.to);
                }
            }
        }
        None
    }

    /// Says why `at`, found by way of `parent`, is not pure: "it reaches `a` → `b`, which
    /// calls the foreign function `write`, at src/lib.rs:4:5".
    fn told(&self, at: u32, parent: &FxHashMap<u32, (u32, usize)>, why: &Why) -> String {
        let db = self.db;
        let mut chain = vec![at];
        while let Some(&(up, _)) = parent.get(chain.last().unwrap()) {
            chain.push(up);
        }
        chain.reverse();
        // Where the last code of the workspace on the way refers to the next.
        let place = why.place.clone().or_else(|| {
            chain.windows(2).rev().find_map(|pair| {
                if !self.is_own(pair[0]) {
                    return None;
                }
                let (_, edge) = parent[&pair[1]];
                let site = self.nodes[pair[0] as usize].edges[edge].site;
                self.site_location(pair[0], site)
            })
        });
        // The cached function is "it", and what a foreign function or a static is said of
        // is what uses it.
        let named = &chain[1..chain.len() - usize::from(why.of_user)];
        let mut told = match named {
            [] => format!("it {}", why.text),
            _ => {
                let names: Vec<String> =
                    named.iter().map(|&it| describe(db, &self.nodes[it as usize].node)).collect();
                let shown = match names.len() {
                    ..=4 => names.join(" → "),
                    _ => format!("{} → … → {}", names[..2].join(" → "), names[names.len() - 1]),
                };
                format!("it reaches {shown}, which {}", why.text)
            }
        };
        if let Some(place) = place {
            told.push_str(&format!(", at {place}"));
        }
        told
    }

    fn site_location(&self, at: u32, site: Site) -> Option<String> {
        let db = self.db;
        let body = match &self.nodes[at as usize].node {
            Node::Root(func) => db.mir_body((*func).into()).ok()?,
            Node::Instance(instance, _) => match instance.def {
                InstanceDef::Fn(func) => db.mir_body(func.into()).ok()?,
                InstanceDef::Closure(id) => {
                    db.mir_body_for_closure(InternedClosureId::from_id(id)).ok()?
                }
                _ => return None,
            },
            _ => return None,
        };
        let refers = |operand: &Operand| match (site, &operand.kind) {
            (Site::Func(func), _) => fn_item(operand).is_some_and(|(it, _)| it == func),
            (Site::Static(id), OperandKind::Static(it)) => *it == id,
            _ => false,
        };
        for (_, block) in body.basic_blocks.iter() {
            for statement in &block.statements {
                if let StatementKind::Assign(_, rvalue) = &statement.kind
                    && operands(rvalue).into_iter().any(refers)
                {
                    return location(db, body, statement.span);
                }
            }
            let Some(terminator) = &block.terminator else { continue };
            if let TerminatorKind::Call { func, args, .. } = &terminator.kind
                && (refers(func) || args.iter().any(refers))
            {
                return location(db, body, terminator.span);
            }
        }
        None
    }
}

/// The foreign functions that give what they are given alone: the allocator, Rust's own
/// shims and libm.
fn pure_foreign(symbol: &str) -> bool {
    const LIBM: &[&str] = &[
        "acos",
        "asin",
        "atan",
        "atan2",
        "cbrt",
        "ceil",
        "cos",
        "cosh",
        "exp",
        "exp2",
        "expm1",
        "fabs",
        "fdim",
        "floor",
        "fma",
        "fmax",
        "fmin",
        "fmod",
        "hypot",
        "log",
        "log10",
        "log1p",
        "log2",
        "pow",
        "round",
        "sin",
        "sinh",
        "sqrt",
        "tan",
        "tanh",
        "tgamma",
        "lgamma_r",
        "trunc",
        "erf",
        "erfc",
        "ldexp",
        "frexp",
        "nextafter",
        "remainder",
    ];
    symbol.starts_with("__rust_")
        || symbol.starts_with("__rdl_")
        || symbol.starts_with("__rg_")
        || matches!(symbol, "memcpy" | "memmove" | "memset" | "memcmp" | "bcmp" | "strlen")
        || LIBM.iter().any(|it| symbol == *it || symbol.strip_suffix('f') == Some(it))
}

/// What a function of core does that gives another result in another run, where the same
/// values are at other addresses: an address as a number, or pointers formatted, hashed or
/// ordered by theirs. Whether two pointers are equal is the same in every run, so equality is
/// not one. core is otherwise trusted, and its MIR not looked through: in it addresses are
/// also numbers where what it gives doesn't depend on them, as in slice iteration and `memchr`.
fn gives_address(db: &dyn HirDatabase, func: FunctionId) -> Option<&'static str> {
    if !matches!(func.krate(db).data(db).origin, base_db::CrateOrigin::Lang(LangCrateOrigin::Core))
    {
        return None;
    }
    let signature = FunctionSignature::of(db, func);
    let name = signature.name.as_str();
    match func.lookup(db).container {
        ItemContainerId::ModuleId(_) => {
            matches!(name, "pointer_fmt_inner" | "wide_pointer_fmt_inner")
                .then_some("formats an address")
        }
        ItemContainerId::ImplId(id) => {
            if !matches!(db.impl_self_ty(id).skip_binder().kind(), TyKind::RawPtr(..)) {
                return None;
            }
            let Some(trait_ref) = db.impl_trait(id) else {
                return matches!(name, "addr" | "expose_provenance")
                    .then_some("gives an address as a number");
            };
            match TraitSignature::of(db, trait_ref.skip_binder().def_id.0).name.as_str() {
                "Hash" => Some("hashes an address"),
                "PartialOrd" | "Ord" => Some("orders pointers by their addresses"),
                _ => None,
            }
        }
        _ => None,
    }
}

fn diverges(db: &dyn HirDatabase, func: FunctionId) -> bool {
    let output = db.callable_item_signature(func.into()).skip_binder().skip_binder().output();
    matches!(output.kind(), TyKind::Never)
}

fn static_name(db: &dyn HirDatabase, id: StaticId) -> String {
    let mut path = vec![StaticSignature::of(db, id).name.as_str().to_owned()];
    if let ItemContainerId::ModuleId(module) = id.lookup(db).container {
        let mut module = Some(module);
        while let Some(at) = module {
            if let Some(name) = at.name(db) {
                path.push(name.as_str().to_owned());
            }
            module = at.containing_module(db);
        }
    }
    path.push(crate_name(db, id.krate(db)));
    path.reverse();
    path.join("::")
}

fn describe(db: &dyn HirDatabase, node: &Node) -> String {
    match node {
        Node::Root(func) => {
            format!("`{}`", function_name(db, *func, GenericArgs::new_from_slice(&[])))
        }
        Node::Instance(instance, _) => {
            let args = instance.args.as_ref();
            match instance.def {
                InstanceDef::Fn(func) | InstanceDef::VtableShim(func) => {
                    format!("`{}`", function_name(db, func, args))
                }
                InstanceDef::Closure(id) => {
                    let at = InternedClosureId::from_id(id).loc(db);
                    match at.owner {
                        InferBodyId::DefWithBodyId(DefWithBodyId::FunctionId(func))
                            if cached_closure(db, func) == Some(id) =>
                        {
                            format!("`{}`", function_name(db, func, args))
                        }
                        InferBodyId::DefWithBodyId(DefWithBodyId::FunctionId(func)) => {
                            format!("a closure in `{}`", function_name(db, func, args))
                        }
                        _ => "a closure".to_owned(),
                    }
                }
                InstanceDef::Coroutine(_) | InstanceDef::CoroutineByMove(_) => {
                    "an `async` block".to_owned()
                }
                InstanceDef::DropGlue => {
                    let ty = args.as_slice().first().and_then(|it| it.ty());
                    let ty = ty.and_then(|ty| hir_ty::mir::type_name::rustc_type_name(db, ty));
                    format!("the drop of `{}`", ty.unwrap_or_default())
                }
                _ => "a shim".to_owned(),
            }
        }
        Node::Static(id) => format!("the static `{}`", static_name(db, *id)),
        Node::Leaf(name, _) => format!("`{name}`"),
    }
}

/// File, line and column of a place in a body, the file relative to the workspace.
fn location(db: &dyn HirDatabase, body: &MirBody<'_>, span: MirSpan) -> Option<String> {
    let InferBodyId::DefWithBodyId(owner) = body.owner else { return None };
    let (file, line, column) = super::source_location(db, owner, span)?;
    let root = db.file_source_root(file).source_root_id(db);
    let root = db.source_root(root).source_root(db);
    let path = root.path_for_file(&file)?.as_path()?.to_string();
    let workspace = owner.module(db).krate(db).data(db).proc_macro_cwd.to_string();
    let path = path
        .strip_prefix(&workspace)
        .map(|it| it.trim_start_matches('/').to_owned())
        .unwrap_or(path);
    Some(format!("{path}:{line}:{column}"))
}

fn operands(rvalue: &Rvalue) -> Vec<&Operand> {
    match rvalue {
        Rvalue::Use(it)
        | Rvalue::Repeat(it, _)
        | Rvalue::Cast(_, it, _)
        | Rvalue::UnaryOp(_, it) => {
            vec![it]
        }
        Rvalue::BinaryOp(_, a, b) | Rvalue::CheckedBinaryOp(_, a, b) => vec![a, b],
        Rvalue::Aggregate(_, it) => it.iter().collect(),
        _ => Vec::new(),
    }
}

/// The first use of a raw pointer in `body` that makes what it gives depend on more than
/// its arguments: a dereference, or an address taken as a number.
fn raw_use(body: &MirBody<'_>) -> Option<(&'static str, MirSpan)> {
    let raw = |place: &hir_ty::mir::StoredPlace| {
        matches!(place.projection.as_slice().first(), Some(ProjectionElem::Deref))
            && matches!(body.locals[place.local].ty.as_ref().kind(), TyKind::RawPtr(..))
    };
    let operand_raw = |operand: &Operand| match &operand.kind {
        OperandKind::Copy(place) | OperandKind::Move(place) => raw(place),
        _ => false,
    };
    for (_, block) in body.basic_blocks.iter() {
        for statement in &block.statements {
            let StatementKind::Assign(place, rvalue) = &statement.kind else {
                if let StatementKind::InlineAsm(_) = &statement.kind {
                    return Some(("has assembly in it", statement.span));
                }
                continue;
            };
            if let Rvalue::Cast(CastKind::PointerExposeAddress, ..) = rvalue {
                return Some(("takes an address as a number", statement.span));
            }
            let read = match rvalue {
                Rvalue::Ref(_, it)
                | Rvalue::Len(it)
                | Rvalue::Discriminant(it)
                | Rvalue::CopyForDeref(it) => raw(it),
                _ => operands(rvalue).into_iter().any(operand_raw),
            };
            if raw(place) || read {
                return Some(("dereferences a raw pointer", statement.span));
            }
        }
    }
    None
}

/// Why the arguments of the closure can't be what a result is kept by.
fn arguments_trouble(
    db: &dyn HirDatabase,
    closure: &Instance,
    env: &StoredParamEnvAndCrate,
) -> Option<String> {
    let types = super::body_types(db, closure.clone(), env.clone()).as_ref().ok()?;
    let interner = DbInterner::new_with(db, env.krate);
    let mut seen = FxHashSet::default();
    // The first parameter is the closure itself, which captures nothing.
    for ty in types.params.iter().skip(1) {
        let ty = ty.as_ref();
        if let Some(why) = trouble(db, interner, ty, false, &mut seen) {
            let name = hir_ty::mir::type_name::rustc_type_name(db, ty).unwrap_or_default();
            return Some(format!("an argument of type `{name}` {why}"));
        }
    }
    None
}

/// Why a value of `ty` can't be what a result is kept by: it can change behind a shared
/// reference, or it holds something the bytes it is written as don't tell all of.
fn trouble<'db>(
    db: &'db dyn HirDatabase,
    interner: DbInterner<'db>,
    ty: Ty<'db>,
    in_library: bool,
    seen: &mut FxHashSet<Ty<'db>>,
) -> Option<String> {
    if !seen.insert(ty) {
        return None;
    }
    match ty.kind() {
        TyKind::Bool
        | TyKind::Char
        | TyKind::Int(_)
        | TyKind::Uint(_)
        | TyKind::Float(_)
        | TyKind::Str
        | TyKind::Never
        | TyKind::FnDef(..)
        | TyKind::FnPtr(..) => None,
        TyKind::Ref(_, pointee, _) => trouble(db, interner, pointee, in_library, seen),
        TyKind::RawPtr(pointee, _) if in_library => trouble(db, interner, pointee, true, seen),
        TyKind::RawPtr(..) => Some("holds a raw pointer".to_owned()),
        TyKind::Array(element, _) | TyKind::Slice(element) | TyKind::Pat(element, _) => {
            trouble(db, interner, element, in_library, seen)
        }
        TyKind::Tuple(fields) => {
            fields.iter().find_map(|field| trouble(db, interner, field, in_library, seen))
        }
        TyKind::Adt(def, args) => {
            let lang = interner.lang_items();
            let library = def.def_id().module(db).krate(db).data(db).origin.is_lang();
            let variants: Vec<VariantId> = match def.def_id() {
                AdtId::StructId(id) if Some(id) == lang.UnsafeCell => {
                    return Some(
                        "can change behind a shared reference: it holds an `UnsafeCell`, as \
                         `Cell`, `RefCell`, `Mutex`, `Rc`, `Arc` and atomics do"
                            .to_owned(),
                    );
                }
                AdtId::StructId(id) => vec![id.into()],
                AdtId::EnumId(id) => {
                    id.enum_variants(db).variants.iter().map(|(_, it)| it.0.into()).collect()
                }
                AdtId::UnionId(_) if !library => return Some("holds a union".to_owned()),
                AdtId::UnionId(id) => vec![id.into()],
            };
            let in_fields = variants.into_iter().find_map(|variant| {
                db.field_types(variant).iter().find_map(|(_, field)| {
                    let ty = field.ty().instantiate(interner, args).skip_norm_wip();
                    trouble(db, interner, ty, library, seen)
                })
            });
            in_fields.or_else(|| {
                // What a marker holds a type for.
                args.iter()
                    .filter_map(|it| it.ty())
                    .find_map(|ty| trouble(db, interner, ty, library, seen))
            })
        }
        TyKind::Dynamic(..) => Some("is a trait object".to_owned()),
        TyKind::Alias(..) if in_library => None,
        _ => Some("is of a type ra-jit can't see into".to_owned()),
    }
}

/// Hashes compiled code without what tells where it was written: the lines it is of and
/// where its variables are.
fn hash_code(hasher: &mut impl Hasher, code: &Code) {
    let Code { signature, bytes, align, relocs, elf_types, unwind, lines: _, slots: _ } = code;
    (signature, bytes, align, elf_types).hash(hasher);
    format!("{unwind:?}").hash(hasher);
    relocs.len().hash(hasher);
    for reloc in relocs {
        (reloc.offset, std::mem::discriminant(&reloc.kind), reloc.addend).hash(hasher);
        std::mem::discriminant(&reloc.name).hash(hasher);
        match &reloc.name {
            ModuleRelocTarget::User { namespace, index } => (namespace, index).hash(hasher),
            ModuleRelocTarget::LibCall(call) => call.hash(hasher),
            ModuleRelocTarget::KnownSymbol(symbol) => symbol.hash(hasher),
            ModuleRelocTarget::FunctionOffset(func, offset) => (func, offset).hash(hasher),
        }
    }
}

impl<'db> FunctionCx<'_, 'db> {
    /// A call of `__ra_jit_cache_code::<F>`: writes the code of the closure `F` to `dst`, as
    /// the `Code` of `ra_jit::__private` has it. That is two words of hash, then the name of
    /// the function, or why it can't be cached when the hash is zero.
    pub(super) fn cache_code(&mut self, generic_args: GenericArgs<'db>, dst: Value) -> Result<()> {
        let db = self.cg.db;
        let ty = generic_args.type_at(0);
        let TyKind::Closure(id, args) = ty.kind() else {
            return Err(CodegenError::Unsupported(format!("`{CODE}` of `{ty:?}`")));
        };
        let parent = GenericArgs::new_from_slice(args.as_closure().parent_args());
        let parent = super::erase_regions(self.interner, parent);
        let closure = Instance {
            def: InstanceDef::Closure(id.0.as_id()),
            args: parent.store(),
            checked: self.cg.checked,
        };
        let env = Homes::default().home(db, &closure, &self.env.store());
        let (hash, text) = match cache_code(db, closure.clone(), env) {
            Ok(hash) => (*hash, cached_name(db, &closure)),
            Err(why) => {
                tracing::info!(target: "ra_jit", "{why}");
                (0, why.clone())
            }
        };
        let words = [hash as u64, (hash >> 64) as u64];
        for (i, word) in words.into_iter().enumerate() {
            let word = self.builder.ins().iconst(types::I64, word as i64);
            self.builder.ins().store(MEM, word, dst, 8 * i as i32);
        }
        let len = self.builder.ins().iconst(ptr(), text.len() as i64);
        let data = self.cg.define_const(ConstBlob {
            bytes: text.into_bytes(),
            relocs: Vec::new(),
            fn_relocs: Vec::new(),
            align: 1,
        })?;
        let data = self.cg.declare_data_in_func(data, self.builder.func);
        let text = self.builder.ins().symbol_value(ptr(), data);
        let width = ptr().bytes() as i32;
        self.builder.ins().store(MEM, text, dst, 16);
        self.builder.ins().store(MEM, len, dst, 16 + width);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cranelift_codegen::{ir::Signature, isa::CallConv};

    use super::super::debuginfo::{LineRow, SlotPlaces};
    use super::*;

    fn code(bytes: &[u8], lines: Vec<LineRow>) -> Code {
        Code {
            signature: Signature::new(CallConv::SystemV),
            bytes: bytes.to_vec(),
            align: 16,
            relocs: Vec::new(),
            elf_types: Vec::new(),
            unwind: None,
            lines,
            slots: SlotPlaces::default(),
        }
    }

    fn hash(code: &Code) -> u128 {
        let mut hasher = rustc_stable_hash::StableSipHasher128::new();
        hash_code(&mut hasher, code);
        let type_id::Fingerprint(low, high) = hasher.finish();
        u128::from(low) | u128::from(high) << 64
    }

    #[test]
    fn the_hash_of_code_is_the_same_in_every_run() {
        // A hash of no state of the process: the one of this code is the one every run, and
        // every daemon, has worked out.
        assert_eq!(
            hash(&code(&[0x90, 0xc3], Vec::new())),
            0x98ba_8c38_257c_5f9e_e4b6_1b43_6ca7_2516
        );
    }

    #[test]
    fn code_moved_to_other_lines_has_the_same_hash_and_other_code_another() {
        let here = code(&[0x90, 0xc3], vec![LineRow { offset: 0, span: 3 }]);
        let moved = code(&[0x90, 0xc3], vec![LineRow { offset: 0, span: 9 }]);
        let other = code(&[0x90, 0x90, 0xc3], Vec::new());
        assert_eq!(hash(&here), hash(&moved));
        assert_ne!(hash(&here), hash(&other));
    }
}
