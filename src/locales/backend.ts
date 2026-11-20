import { msg } from "@/i18n";

/**
 * The text the Rust side shows (the tray menu and its tooltip, notifications, the DPI pop-up),
 * listed here so `pnpm strings` keeps it in the language files, which `src-tauri/src/i18n.rs` reads
 * too. Its test `every_key_is_listed` fails until each `tr("…")` there is in this list.
 */
export const BACKEND_TEXT = [
  // The tray menu and its tooltip.
  msg("Open hrtz"),
  msg("Quit"),
  msg("Restart to update"),
  msg("Profile {{n}}"),
  msg("Mouse"),
  msg("{{model}}: asleep"),
  msg("{{level}}% charging"),
  msg("hrtz: no mouse connected"),
  // Notifications.
  msg("hrtz {{version}} is ready"),
  msg("Choose Restart to update in the tray menu or the sidebar."),
  msg("Mouse battery at {{level}}%"),
  msg("Charge your {{model}} soon."),
  msg("Battery saver lowered polling to {{rate}}"),
  msg("Battery at {{level}}%. Your {{model}} returns to {{rate}} when charged."),
  // The DPI and profile pop-up.
  msg("Stage {{stage}}"),
  msg("Polling rate"),
];
