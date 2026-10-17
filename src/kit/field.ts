/**
 * The fill and edge of a Fluent field, for shadcn input groups that sit among Fluent controls:
 * Fluent's own theme variables, so light and dark both match the Fluent buttons beside them.
 * Includes `tw`, the scope shadcn parts need (src/shadcn.css). Put it on an InputGroup.
 */
export const fieldLook = "tw border-[color:var(--colorNeutralStroke1)] bg-[var(--colorNeutralBackground1)] dark:bg-[var(--colorNeutralBackground1)]";

/** Controls are this tall, Fluent's medium size. Icon-only Fluent buttons need telling (they're
 *  28px here, with the app's 16px button glyphs). */
export const CONTROL_HEIGHT = 32;

/** The window's own title bar (TitleBar.tsx), which flyouts must stay below. */
export const TITLE_BAR_HEIGHT = 32;

/**
 * Room to keep between a flyout and the window's edges: below the title bar at the top, 8px
 * elsewhere. Give it to Fluent positioning as `overflowBoundaryPadding`, with `autoSize`, on
 * flyouts that can grow tall, so they shrink to fit instead of running under the title bar.
 */
export const FLYOUT_PADDING = { top: TITLE_BAR_HEIGHT + 8, end: 8, bottom: 8, start: 8 };
