// Re-exports for backwards compatibility. These items now live in asset_browser.
// `ApplyTextureToFaces` was replaced by the `material.apply_texture` operator;
// call it via `commands.operator("material.apply_texture").param("path", path).call()`.
pub use crate::asset_browser::ClearTextureFromFaces;
