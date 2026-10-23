#!/usr/bin/env python3
"""cpufold.py SCRIPT [--comm NAME] [--dso PATH]: what share of a `perf` profile's CPU time parsing and lowering take.

SCRIPT is what `perf script` prints of a `perf record -g` of the daemon (frame pointers:
`RUSTFLAGS=-Cforce-frame-pointers=yes`). Two tables. The first gives, for each kind of work,
the share of the samples with a frame of that kind on their stack: a parse asked for by an
item tree counts in both. The second gives each sample to one kind, that of its innermost
frame that is of a kind: what the time was spent doing itself, which adds up to 100%.
`--comm` keeps the samples of processes whose name starts with NAME (default `ra-jit`), and
`--dso PATH` those with a frame in a binary whose path has PATH in it, for a profile of the
whole machine (`perf record -a`) that other ra-jit builds ran beside.
"""
import collections, re, sys

# Of what is on the stack.
KINDS = [
    ("parse a file", ("SourceFile::parse", "syntax::parsing::parse_text")),
    ("parse a macro expansion", ("token_tree_to_syntax_node",)),
    ("ast id maps", ("AstIdMap>::from_source",)),
    ("span maps", ("real_span_map",)),
    ("item trees", ("hir_def::item_tree::lower", "file_item_tree_query", "block_item_tree_query")),
    ("body syntax (ra-jit)", ("hir_def::expr_store::body::body_syntax",)),
    ("a body's anchor parsed again", ("BodySyntax>::reparse", "BodySyntax::reparse")),
    ("places in bodies (line tables, panic locations)", ("hir_ty_codegen::places::",)),
    ("bodies and signatures lowered", ("hir_def::expr_store::lower",)),
    ("macro expansion", ("hir_expand::", "mbe::", "proc_macro_api::")),
    ("name resolution", ("hir_def::nameres::collector",)),
    ("inference", ("hir_ty::infer::",)),
    ("MIR building", ("hir_ty::mir::lower",)),
    ("code generation", ("hir_ty_codegen::", "cranelift", "llvm")),
]

# What a frame itself does, the first that matches.
OWN = [
    ("flat trees built (rowan's flat builder)", ("FlatTreeBuilder", "rowan::flat::FlatTree>::subtree")),
    ("green trees built (rowan's builder and node cache)", ("GreenNodeBuilder", "node_cache::NodeCache", "SyntaxTreeBuilder", "rowan::green::")),
    ("trees walked (rowan's cursor, green or flat)", ("rowan::cursor", "rowan::api", "rowan::flat", "<rowan::")),
    ("lexed and parsed (events)", ("parser::", "LexedStr", "rustc_lexer")),
    ("anchors hashed", ("hir_def::expr_store::body::tree_hash",)),
    ("ast id maps", ("span::ast_id::",)),
    ("span maps", ("span::map::", "real_span_map")),
    ("token trees (syntax to and from)", ("syntax_bridge::", "tt::")),
    ("macro expansion", ("mbe::", "proc_macro_api::", "hir_expand::")),
    ("item trees", ("hir_def::item_tree",)),
    ("body syntax (ra-jit)", ("hir_def::expr_store::body::",)),
    ("bodies and signatures lowered", ("hir_def::expr_store::lower",)),
    ("name resolution", ("hir_def::nameres",)),
    ("inference and traits", ("hir_ty::infer", "hir_ty::next_solver", "rustc_next_trait_solver", "rustc_type_ir")),
    ("MIR", ("hir_ty::mir",)),
    ("code generation", ("hir_ty_codegen::", "cranelift", "llvm", "ra_jit_codegen")),
    ("salsa", ("salsa::",)),
]


def samples(path, comm, dso=None):
    """Each sample's frames, innermost first: of processes named `comm`, and with `dso` in the
    path of one of the frames' binaries when it is given."""
    frames, keep, of_dso = [], False, dso is None
    for line in open(path, errors="replace"):
        if not line.strip():
            if keep and of_dso and frames:
                yield frames
            frames, keep, of_dso = [], False, dso is None
        elif not line[0].isspace():
            keep = line.split()[0].startswith(comm)
        elif keep:
            # "    addr symbol+0xoff (dso)"
            parts = line.strip().split(" ", 1)
            of_dso = of_dso or dso in line
            frames.append(re.sub(r"\+0x[0-9a-f]+ \(.*\)$|\s\(.*\)$", "", parts[1]) if len(parts) > 1 else parts[0])
    if keep and of_dso and frames:
        yield frames


def own(frames):
    for frame in frames:
        for label, needles in OWN:
            if any(it in frame for it in needles):
                return label
    return "other (allocator, kernel, std, unknown)"


def main():
    comm = sys.argv[sys.argv.index("--comm") + 1] if "--comm" in sys.argv else "ra-jit"
    dso = sys.argv[sys.argv.index("--dso") + 1] if "--dso" in sys.argv else None
    total = 0
    on_stack, by_own = collections.Counter(), collections.Counter()
    for frames in samples(sys.argv[1], comm, dso):
        total += 1
        joined = ";".join(frames)
        for label, needles in KINDS:
            if any(it in joined for it in needles):
                on_stack[label] += 1
        by_own[own(frames)] += 1
    print(f"{total} samples\non the stack:")
    for label, _ in KINDS:
        print(f"{100 * on_stack[label] / max(1, total):6.2f}%  {label}")
    print("by what the innermost frame of a kind does:")
    for label, count in by_own.most_common():
        print(f"{100 * count / max(1, total):6.2f}%  {label}")


if __name__ == "__main__":
    main()
