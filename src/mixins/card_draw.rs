// SPDX-License-Identifier: MIT
// Frontier's settings page: its card is drawn instead of Sunrise's generic page.
fn frontier_card_options_draw(
    game: &mut crate::game::Game<'_>,
    batch: &mut sunrise_client::hud::HudBatch,
    allowed: bool,
) -> ::core::ops::ControlFlow<()> {
    if !allowed || !crate::game::frontier_card::active(game) {
        return ::core::ops::ControlFlow::Continue(());
    }
    crate::game::frontier_card::draw(game, batch);
    ::core::ops::ControlFlow::Break(())
}
