import { Button, makeStyles, Spinner, tokens } from "@fluentui/react-components";
import { ArrowDownload16Regular, ArrowSync16Regular, Checkmark16Regular, Warning16Regular } from "@fluentui/react-icons";
import { openUrl } from "@tauri-apps/plugin-opener";
import { IconButton } from "@/kit";
import { REPO_URL, useUpdate } from "./update";

const useStyles = makeStyles({
  // Pinned to the bottom of the sidebar, under the pages.
  row: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: tokens.spacingHorizontalS,
    marginTop: "auto",
  },
  // Ghost, in the nav rows' weight, its icon centred under theirs.
  update: {
    paddingLeft: "14px",
    fontWeight: tokens.fontWeightRegular,
    color: tokens.colorNeutralForeground2,
    minWidth: 0,
  },
  // The soft marker What's new uses for something not seen yet.
  dot: {
    width: "6px",
    height: "6px",
    marginLeft: tokens.spacingHorizontalS,
    borderRadius: tokens.borderRadiusCircular,
    backgroundColor: "var(--accent)",
  },
});

/** GitHub's mark (Octicons' mark-github, MIT), drawn in the text colour. */
function GitHubMark() {
  return (
    <svg viewBox="0 0 16 16" width={16} height={16} fill="currentColor" aria-hidden>
      <path d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
    </svg>
  );
}

/**
 * The bottom of the sidebar: the update check, and hrtz on GitHub. The check runs quietly too, so
 * the button turns into "Update to …" by itself when a release is out.
 */
export function SidebarFooter() {
  const s = useStyles();
  const { state, check } = useUpdate();
  const open = (url: string) => void openUrl(url).catch(() => {});

  const [label, icon, title] =
    state.status === "checking"
      ? ["Checking…", <Spinner size="extra-tiny" />, undefined]
      : state.status === "current"
        ? ["Up to date", <Checkmark16Regular />, undefined]
        : state.status === "failed"
          ? ["Couldn't check", <Warning16Regular />, state.reason]
          : state.status === "available"
            ? [`Update to ${state.release.version}`, <ArrowDownload16Regular />, "Opens the release on GitHub"]
            : ["Check for updates", <ArrowSync16Regular />, undefined];

  return (
    <div className={s.row}>
      <Button
        appearance="subtle"
        className={s.update}
        icon={icon}
        title={title}
        disabled={state.status === "checking"}
        onClick={() => (state.status === "available" ? open(state.release.url) : void check())}
      >
        {label}
        {state.status === "available" && <span aria-hidden className={s.dot} />}
      </Button>
      <IconButton appearance="subtle" icon={<GitHubMark />} label="hrtz on GitHub" onClick={() => open(REPO_URL)} />
    </div>
  );
}
