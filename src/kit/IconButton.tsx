import { Button, type ButtonProps } from "@fluentui/react-components";
import { CONTROL_HEIGHT } from "./field";

/**
 * A Fluent icon-only button at the app's control height (32px; a plain one is 28px here). `label`
 * is its name for screen readers and its tooltip. Passes everything else to Fluent's Button, so
 * it works as a MenuTrigger child.
 */
export function IconButton({ label, style, ...props }: ButtonProps & { label: string }) {
  return <Button aria-label={label} title={label} {...props} style={{ height: CONTROL_HEIGHT, minWidth: CONTROL_HEIGHT, ...style }} />;
}
