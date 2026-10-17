import { webDarkTheme, webLightTheme, type Theme } from "@fluentui/react-components";

// Windows 11 ships Segoe UI Variable; Fluent's web ramp asks for plain Segoe UI.
const font = "'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif";

/**
 * Fluent web themes with WinUI control styling: the near-invisible ControlStrokeColorDefault border
 * and ControlFill hover/pressed fills, so buttons, toggles and inputs look like Windows Settings and
 * PowerToys instead of Fluent web's hard #666 outline.
 */
function winui(t: Theme, isDark: boolean): Theme {
  const stroke = isDark ? "rgba(255, 255, 255, 0.0706)" : "rgba(0, 0, 0, 0.0578)";
  return {
    ...t,
    fontFamilyBase: font,
    colorNeutralStroke1: stroke,
    colorNeutralStroke1Hover: stroke,
    colorNeutralStroke1Pressed: stroke,
    colorNeutralStroke1Selected: stroke,
    // ControlFillColorDisabled: a faint wash, not an opaque block on Mica.
    colorNeutralBackgroundDisabled: isDark ? "rgba(255, 255, 255, 0.0419)" : "rgba(249, 249, 249, 0.3)",
    colorNeutralStrokeDisabled: stroke,
    // One focus ring for every control (states.css, values from states.json).
    colorStrokeFocus1: "transparent",
    colorStrokeFocus2: "var(--focus-ring)",
    ...(isDark ? { colorNeutralBackground1Hover: "#323232", colorNeutralBackground1Pressed: "#282828" } : {}),
  };
}

export const light = winui(webLightTheme, false);
export const dark = winui(webDarkTheme, true);
