// SPDX-License-Identifier: MIT
// The mouse buttons on Frontier's settings card.
fn frontier_card_options_mouse(
    game: &mut crate::game::Game<'_>,
    pressed: bool,
    right: bool,
) -> ::core::ops::ControlFlow<Result<()>> {
    if !crate::game::frontier_card::active(game) {
        return ::core::ops::ControlFlow::Continue(());
    }
    ::core::ops::ControlFlow::Break(crate::game::frontier_card::mouse(game, pressed, right))
}
