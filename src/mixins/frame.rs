// Once a frame: the settings page, when asked for.
fn frontier_frame(game: &mut crate::game::Game<'_>) {
    crate::game::frontier_glue::frame(game);
}
