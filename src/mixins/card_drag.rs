// SPDX-License-Identifier: MIT
// Dragging on Frontier's settings card: its sliders and scrollbar.
fn frontier_card_options_drag(game: &mut crate::game::Game<'_>) -> ::core::ops::ControlFlow<Result<()>> {
    if !crate::game::frontier_card::active(game) {
        return ::core::ops::ControlFlow::Continue(());
    }
    ::core::ops::ControlFlow::Break(crate::game::frontier_card::drag(game))
}
