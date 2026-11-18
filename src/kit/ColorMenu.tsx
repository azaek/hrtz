import { Menu, MenuGroup, MenuGroupHeader, MenuItemRadio, MenuList, MenuPopover, MenuTrigger } from "@fluentui/react-components";
import { cn } from "@/lib/utils";
import { t } from "@/i18n";
import { IconButton } from "./IconButton";

export type Swatch = { name: string; hex: string };

/** A colour dot, with a faint inner edge so light colours hold their shape. */
export function Dot({ color, className }: { color: string; className?: string }) {
  return <span aria-hidden className={cn("inline-block size-2.5 shrink-0 rounded-full ring-1 ring-black/25 ring-inset", className)} style={{ backgroundColor: color }} />;
}

/** A colour's name, in the language shown (a palette's names are `msg()` keys); its hex if unnamed. */
export const swatchName = (palette: Swatch[], hex: string) => {
  const name = palette.find((p) => p.hex === hex.toUpperCase())?.name;
  return name ? t(name) : hex.toUpperCase();
};

/**
 * Pick one colour from `palette`: a 32px Fluent button showing the colour, and a Fluent menu of
 * the palette (dot and name each) with the current one checked. A colour outside the palette
 * still shows on the button, with nothing checked.
 */
export function ColorMenu({
  color,
  label,
  palette,
  disabled,
  onChange,
}: {
  color: string;
  label: string;
  palette: Swatch[];
  disabled?: boolean;
  onChange: (hex: string) => void;
}) {
  return (
    <Menu positioning="below-start" checkedValues={{ colour: [color.toUpperCase()] }} onCheckedValueChange={(_, d) => d.checkedItems[0] && onChange(d.checkedItems[0])}>
      <MenuTrigger disableButtonEnhancement>
        <IconButton icon={<Dot color={color} className="size-3.5" />} label={`${label}: ${swatchName(palette, color)}`} disabled={disabled} />
      </MenuTrigger>
      <MenuPopover>
        <MenuList>
          <MenuGroup>
            <MenuGroupHeader>{label}</MenuGroupHeader>
            {palette.map((p) => (
              <MenuItemRadio key={p.hex} name="colour" value={p.hex} icon={<Dot color={p.hex} className="size-3" />}>
                {t(p.name)}
              </MenuItemRadio>
            ))}
          </MenuGroup>
        </MenuList>
      </MenuPopover>
    </Menu>
  );
}
