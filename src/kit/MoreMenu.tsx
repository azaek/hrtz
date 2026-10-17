import { Fragment, type ReactElement } from "react";
import { Menu, MenuDivider, MenuItem, MenuList, MenuPopover, MenuTrigger } from "@fluentui/react-components";
import { MoreHorizontal20Regular } from "@fluentui/react-icons";
import { IconButton } from "./IconButton";

/** Run `f` once a Fluent menu has closed and handed focus back to its button, so a field that a
 *  menu item opens (Rename) keeps its focus. */
export const afterMenu = (f: () => void) => window.setTimeout(f, 0);

export type MoreMenuItem = { label: string; icon?: ReactElement; onClick: () => void; disabled?: boolean } | "divider";

/**
 * A row's ⋯ menu: Fluent's Menu behind a subtle 32px icon button. Pass the *16Regular icons (menu
 * glyphs are 16px, like button glyphs). Hover and press are the app's soft states (base.css);
 * dividers are the card's hairline.
 */
export function MoreMenu({ label, items }: { label: string; items: MoreMenuItem[] }) {
  return (
    <Menu positioning="below-end">
      <MenuTrigger disableButtonEnhancement>
        <IconButton appearance="subtle" icon={<MoreHorizontal20Regular />} label={label} />
      </MenuTrigger>
      <MenuPopover>
        <MenuList>
          {items.map((item, i) =>
            item === "divider" ? (
              <MenuDivider key={i} style={{ backgroundColor: "var(--border)" }} />
            ) : (
              <Fragment key={item.label}>
                <MenuItem icon={item.icon} disabled={item.disabled} onClick={item.onClick}>
                  {item.label}
                </MenuItem>
              </Fragment>
            ),
          )}
        </MenuList>
      </MenuPopover>
    </Menu>
  );
}
