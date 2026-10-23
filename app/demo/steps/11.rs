//! The hot-swappable half of the app: a game of Tetris.
//!
//! Everything in this module — the palette, the board's look, the rules,
//! the controls — is patch territory: edit and save, the daemon splices
//! the new code into the running process (about a millisecond to apply)
//! and the `dynamic` under [`crate::App`] rebuilds this view. The game
//! itself keeps playing through it all: its state is a plain `thread_local`
//! in the never-restarted process, the gravity tick an interval owned by
//! the root, and only the *view* of the game is remounted. `keep`ed
//! signals (the high score) additionally survive the remount through the
//! keep table.
//!
//! What ra-jit cannot do it says instead: an edit that changes the layout
//! of a type the program can see is answered `Restart` and the status
//! line shows it — the process keeps the old code until started again.
//!
//! One more constraint learned the hard way: a patch containing
//! thread-local data cannot splice on the iOS simulator (the link
//! completes but is "not spliced: the patch has thread-local data", the
//! binary relinks, and the running sim process is orphaned to
//! restart-needed forever). And a `static` here is just as wrong: local
//! statics carry no name the splice can match, so every patch emits a
//! private copy — patched code would see an empty board. State therefore
//! lives behind `live`'s accessors, in a file no demo edits; this file
//! holds only code on it, which is what a patch redirects.

use nsh_core::ui::prelude::*;

use crate::live::keep;

// ---------------------------------------------------------------------------
// The knobs a demo edits first: what the board looks like and how it plays.

/// An [`AdaptiveColor`] from `hsla` parts.
const fn tone(light: Hsla, dark: Hsla) -> AdaptiveColor {
    AdaptiveColor { light, dark }
}

/// `Hsla` with no clamping: values are ours to keep in `0..=1`.
const fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla { h, s, l, a }
}

/// One palette entry, the same in light and dark appearance: the pieces
/// keep their colors in both.
const fn flat(h: f32, s: f32, l: f32) -> AdaptiveColor {
    tone(hsla(h, s, l, 1.0), hsla(h, s, l, 1.0))
}

const ACCENT: AdaptiveColor = tone(rgb(0x2e6fc2).to_hsla(), rgb(0x7fb3f0).to_hsla());
const CARD: AdaptiveColor = tone(hsla(0.0, 0.0, 1.0, 1.0), hsla(0.62, 0.09, 0.11, 1.0));
const BORDER: AdaptiveColor = tone(hsla(0.15, 0.07, 0.86, 1.0), hsla(0.60, 0.08, 0.19, 1.0));
const TEXT: AdaptiveColor = tone(hsla(0.60, 0.10, 0.09, 1.0), hsla(0.58, 0.24, 0.94, 1.0));
const MUTED: AdaptiveColor = tone(hsla(0.60, 0.06, 0.40, 1.0), hsla(0.60, 0.07, 0.59, 1.0));
const GRID: AdaptiveColor = tone(hsla(0.20, 0.05, 0.88, 1.0), hsla(0.60, 0.08, 0.17, 1.0));
const UP: AdaptiveColor = tone(hsla(0.42, 0.75, 0.31, 1.0), hsla(0.42, 0.67, 0.60, 1.0));

/// What a cell holds: 0 empty, 1..=7 the seven pieces, 8 a ghost.
/// The palette is the place a demo recolors the whole game with one edit.
const CELLS: [AdaptiveColor; 8] = [
    flat(0.63, 0.40, 0.30), // 0 empty
    flat(0.55, 1.00, 0.52), // 1 I — aqua
    flat(0.50, 0.95, 0.58), // 2 O — gold
    flat(0.75, 0.90, 0.62), // 3 T — magenta
    flat(0.38, 0.85, 0.60), // 4 S — lime
    flat(0.98, 0.85, 0.60), // 5 Z — coral
    flat(0.55, 0.90, 0.65), // 6 J — sky
    flat(0.05, 0.90, 0.60), // 7 L — tangerine
];

/// Board geometry: ten columns, twenty rows, twenty-pixel cells.
pub const W: usize = 10;
pub const H: usize = 20;
const CELL: f32 = 14.0;

/// Gravity: the piece falls one row every `GRAVITY` ticks of the 50 ms
/// game clock — so 8 here is a row every 0.4 s. Editing this changes the
/// speed of the game in progress, mid-fall.
const GRAVITY: u64 = 16;

/// Points per 0, 1, 2, 3, 4 lines cleared at once (times level).
const SCORES: [u32; 5] = [0, 40, 100, 300, 1200];

// ---------------------------------------------------------------------------
// The game: plain process state and plain functions on it.

/// Where the seven pieces start from: cells as (x, y) in their box.
const SHAPES: [[(i32, i32); 4]; 7] = [
    [(0, 1), (1, 1), (2, 1), (3, 1)], // I
    [(0, 0), (1, 0), (0, 1), (1, 1)], // O
    [(0, 1), (1, 1), (2, 1), (1, 0)], // T
    [(1, 0), (2, 0), (0, 1), (1, 1)], // S
    [(0, 0), (1, 0), (1, 1), (2, 1)], // Z
    [(0, 0), (0, 1), (1, 1), (2, 1)], // J
    [(2, 0), (0, 1), (1, 1), (2, 1)], // L
];

/// `kind`'s cells turned `rot` quarter turns, inside its box.
const fn cells_of(kind: u8, rot: u8) -> [(i32, i32); 4] {
    let mut cs = SHAPES[kind as usize];
    if kind == 1 {
        return cs; // O does not rotate
    }
    let n = if kind == 0 { 4 } else { 3 };
    let mut r = 0;
    while r < rot % 4 {
        let mut i = 0;
        while i < 4 {
            let (x, y) = cs[i];
            cs[i] = (n - 1 - y, x);
            i += 1;
        }
        r += 1;
    }
    cs
}

#[derive(Clone, Copy)]
struct Piece {
    kind: u8,
    rot: u8,
    x: i32,
    y: i32,
}

pub(crate) struct Game {
    board: [[u8; W]; H],
    cur: Piece,
    next: u8,
    score: u32,
    lines: u32,
    over: bool,
    rng: u64,
    tick_n: u64,
}

impl Game {
    /// `const` so `live`'s `static GAME: Mutex<Game>` can link-time it.
    pub(crate) const fn new() -> Self {
        let mut g = Self {
            board: [[0; W]; H],
            cur: Piece { kind: 0, rot: 0, x: 0, y: 0 },
            next: 0,
            score: 0,
            lines: 0,
            over: false,
            rng: 0x9E37_79B9_7F4A_7C15,
            tick_n: 0,
        };
        g.next = g.rand7();
        g.spawn();
        g
    }

    const fn rand7(&mut self) -> u8 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % 7) as u8
    }

    fn level(&self) -> u32 {
        self.lines / 10 + 1
    }

    /// Do the cells of `kind` at `rot`, offset `(x, y)`, sit in free space?
    const fn fits(&self, kind: u8, rot: u8, x: i32, y: i32) -> bool {
        let cs = cells_of(kind, rot);
        let mut i = 0;
        while i < 4 {
            let (bx, by) = (x + cs[i].0, y + cs[i].1);
            if bx < 0
                || bx >= W as i32
                || by >= H as i32
                || (by >= 0 && self.board[by as usize][bx as usize] != 0)
            {
                return false;
            }
            i += 1;
        }
        true
    }

    const fn spawn(&mut self) {
        // A random column each spawn — with nobody steering, pieces
        // spread across the board instead of towering up the middle.
        let x = self.rand7() as i32;
        self.cur = Piece {
            kind: self.next,
            rot: 0,
            x,
            y: 0,
        };
        self.next = self.rand7();
        if !self.fits(self.cur.kind, self.cur.rot, self.cur.x, self.cur.y) {
            self.over = true;
        }
    }

    /// One row down, or lock in place and bring the next piece.
    fn gravity(&mut self) {
        if self.fits(self.cur.kind, self.cur.rot, self.cur.x, self.cur.y + 1) {
            self.cur.y += 1;
        } else {
            self.lock();
        }
    }

    fn lock(&mut self) {
        for (cx, cy) in cells_of(self.cur.kind, self.cur.rot) {
            let (bx, by) = (self.cur.x + cx, self.cur.y + cy);
            if by >= 0 {
                self.board[by as usize][bx as usize] = self.cur.kind + 1;
            }
        }
        let cleared = self.clear_lines();
        self.lines += cleared;
        self.score += SCORES[cleared as usize] * self.level();
        self.spawn();
    }

    fn clear_lines(&mut self) -> u32 {
        let mut w = H;
        let mut cleared = 0u32;
        for y in (0..H).rev() {
            if self.board[y].iter().all(|&c| c != 0) {
                cleared += 1;
            } else {
                w -= 1;
                self.board[w] = self.board[y];
            }
        }
        for y in 0..w {
            self.board[y] = [0; W];
        }
        cleared
    }

    /// What a 4×4 preview box cell holds: the next piece's shape, or empty.
    fn next_cell(&self, x: usize, y: usize) -> u8 {
        if cells_of(self.next, 0).contains(&(x as i32, y as i32)) {
            self.next + 1
        } else {
            0
        }
    }

    /// What shows at board cell (x, y): the falling piece first (and its
    /// ghost, the landing place), then whatever is locked there.
    fn cell(&self, x: usize, y: usize) -> u8 {
        let mut ghost_y = self.cur.y;
        while self.fits(self.cur.kind, self.cur.rot, self.cur.x, ghost_y + 1) {
            ghost_y += 1;
        }
        for (cx, cy) in cells_of(self.cur.kind, self.cur.rot) {
            if self.cur.x + cx == x as i32 && self.cur.y + cy == y as i32 {
                return self.cur.kind + 1;
            }
            if !self.over && self.cur.x + cx == x as i32 && ghost_y + cy == y as i32
                && self.board[y][x] == 0
            {
                return 0; // a ghost: baseline renders it as empty
            }
        }
        self.board[y][x]
    }
}

/// The game and its repaint pulse live behind [`live`]'s accessors:
/// `static`s in this file would be re-emitted privately into every patch,
/// so each generation of code would see an empty board and signal. What
/// stays here is only code on the state — which patches do redirect.
fn frame() -> Signal<u64> {
    crate::live::frame()
}

/// The game clock: gravity every `GRAVITY` calls. Returns whether the
/// board changed — the caller bumps `frame` then, waking the cells.
pub(crate) fn tick() -> bool {
    let mut g = crate::live::game();
    g.tick_n += 1;
    if g.over {
        // Dead board stays dead — only the reset button deals a new
        // game, so a demo can see both apps react to the same press.
        return false;
    }
    if g.tick_n.is_multiple_of(GRAVITY) {
        g.gravity();
        true
    } else {
        false
    }
}

/// Run `f` on the game; bump `frame` if it changed anything.
fn act(f: impl FnOnce(&mut Game) -> bool) {
    let changed = {
        let mut g = crate::live::game();
        f(&mut g)
    };
    if changed {
        frame().update(|v| *v += 1);
    }
}

// --- controls: every one a plain function, every one patch territory -------

pub fn move_left() {
    act(|g| !g.over && g.fits(g.cur.kind, g.cur.rot, g.cur.x - 1, g.cur.y) && {
        g.cur.x -= 1;
        true
    });
}

pub fn move_right() {
    act(|g| !g.over && g.fits(g.cur.kind, g.cur.rot, g.cur.x + 1, g.cur.y) && {
        g.cur.x += 1;
        true
    });
}

pub fn rotate_cw() {
    act(|g| {
        if g.over {
            return false;
        }
        let rot = (g.cur.rot + 1) % 4;
        for dx in [0, -1, 1, -2, 2] {
            if g.fits(g.cur.kind, rot, g.cur.x + dx, g.cur.y) {
                g.cur.rot = rot;
                g.cur.x += dx;
                return true;
            }
        }
        false
    });
}

pub fn soft_drop() {
    act(|g| {
        if g.over {
            return false;
        }
        g.gravity();
        true
    });
}

pub fn hard_drop() {
    act(|g| {
        if g.over {
            return false;
        }
        while g.fits(g.cur.kind, g.cur.rot, g.cur.x, g.cur.y + 1) {
            g.cur.y += 1;
        }
        g.lock();
        true
    });
}

pub fn restart() {
    act(|g| {
        *g = Game::new();
        true
    });
}

// ---------------------------------------------------------------------------
// The view: rebuilt by each patch, reading `GAME` through the `frame` pulse.

/// What the patch demo shows: a game of Tetris that keeps running while
/// its code is replaced. Board, piece, score and `keep`ed high score
/// carry over; colors, rules, layout and controls are live-editable.
#[component]
pub fn HotArea() -> impl IntoElement {
    let high = keep("tetris.high", || 0u32);

    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(14.0))
        .child(
            view()
                .flex_row()
                .items_end()
                .gap(px(14.0))
                .child(
                    view()
                        .gap(px(4.0))
                        .child(
                            text("PATCHED LIVE")
                                .text_size(px(24.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(ACCENT),
                        )
                        .child(
                            text(move || {
                                frame().get();
                                let g = crate::live::game();
                                if g.score > high.get_untracked() {
                                    high.set(g.score);
                                }
                                format!("score {} · lines {} · best {}", g.score, g.lines, high.get_untracked())
                            })
                            .text_size(px(12.0))
                            .font_family("monospace")
                            .text_color(MUTED),
                        ),
                )
                .child(next_box()),
        )
        .child(
            view()
                .bg(CARD)
                .border_1()
                .border_color(BORDER)
                .border_curve(BorderCurve::Continuous)
                .rounded(px(0.0))
                .p(px(6.0))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .w(px(CELL * W as f32))
                        .h(px(CELL * H as f32))
                        .gap(px(1.0))
                        .bg(GRID)
                        .children((0..H * W).map(|i| cell_view(i % W, i / W))),
                ),
        )
        .child(
            view()
                .flex_row()
                .gap(px(10.0))
                .items_center()
                .child(control("◀", move_left))
                .child(control("⟳", rotate_cw))
                .child(control("▶", move_right))
                .child(control("▼", soft_drop))
                .child(control("drop", hard_drop))
                .child(control("reset", restart)),
        )
        .child(
            text(move || {
                frame().get();
                if crate::live::game().over {
                    "game over — reset to play again".to_owned()
                } else {
                    "built live, top to bottom — game already running".to_owned()
                }
            })
            .text_size(px(12.0))
            .text_color(UP),
        )
}

/// A 4×4 preview of the next piece — a component added by a patch:
/// these functions did not exist when the app launched.
#[component]
fn NextBox() -> impl IntoElement {
    let cell = |x: usize, y: usize| {
        let c = computed(move || {
            frame().get();
            crate::live::game().next_cell(x, y)
        });
        view().w(px(11.0)).h(px(11.0)).bg(move || CELLS[c.get() as usize])
    };
    view()
        .flex_col()
        .items_center()
        .gap(px(6.0))
        .child(text("next").text_size(px(11.0)).text_color(MUTED))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .w(px(48.0))
                .h(px(48.0))
                .gap(px(1.0))
                .bg(GRID)
                .children((0..16).map(|i| cell(i % 4, i / 4))),
        )
}

/// One board cell: a fixed-size square whose fill follows its game cell.
fn cell_view(x: usize, y: usize) -> impl IntoElement {
    let at = computed(move || {
        frame().get();
        crate::live::game().cell(x, y)
    });
    view()
        .w(px(CELL - 1.0))
        .h(px(CELL - 1.0))
        .bg(move || CELLS[at.get() as usize])
}

/// A pressable control.
fn control(label: &'static str, press: fn()) -> impl IntoElement {
    view()
        .px_4()
        .py_2()
        .bg(CARD)
        .border_1()
        .border_color(BORDER)
        .rounded(px(0.0))
        .border_curve(BorderCurve::Continuous)
        .on_press(move || press())
        .child(
            text(label)
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(TEXT),
        )
}
