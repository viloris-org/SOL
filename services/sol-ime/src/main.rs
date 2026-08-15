//! sol-ime — SOL system input method bridge (PRD §38 Phase 1 / IME section).
//!
//! Phase 0 placeholder. Per the decided approach (Option A), SOL provides a
//! first-party IME *frontend* (candidate window / preedit, themed with
//! `sol-ui` + `sol-design`) and reuses **fcitx5** as the engine backend,
//! rather than self-hosting a Chinese/pinyin engine.
//!
//! Target mainstream languages first (Chinese pinyin, then others) via
//! fcitx5 engine addons (`fcitx5-chinese-addons` etc.).
//!
//! The public API will be designed during Phase 1 (input-method + text-input
//! protocol integration on the compositor side).

fn main() {
    println!("sol-ime: Phase 0 scaffold, not yet implemented");
}
