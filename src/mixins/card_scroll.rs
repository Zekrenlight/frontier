// SPDX-License-Identifier: MIT
// The wheel on Frontier's settings card.
fn frontier_card_options_scroll(
    game: &mut crate::game::Game<'_>,
    amount: f32,
) -> ::core::ops::ControlFlow<Result<()>> {
    if !crate::game::frontier_card::active(game) {
        return ::core::ops::ControlFlow::Continue(());
    }
    ::core::ops::ControlFlow::Break(crate::game::frontier_card::scroll(game, amount))
}
