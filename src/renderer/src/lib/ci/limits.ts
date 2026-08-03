// Renderer mirror of the CI bounds in src/main/ci/config.ts — the renderer
// cannot import main-process modules, and the configurator both enforces the
// selection cap (Save gating) and names the number in copy, so the value must
// live in exactly one renderer place. Keep in sync with CI_MAX_BUILD_TYPES.
export const CI_MAX_BUILD_TYPES = 50
