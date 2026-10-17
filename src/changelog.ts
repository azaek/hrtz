import raw from "../CHANGELOG.md?raw";

export interface Release {
  version: string;
  /** As written after the version: a date, or "Unreleased". */
  date: string;
  sections: { title: string; items: string[] }[];
}

/**
 * CHANGELOG.md as releases, newest first. Only its own shape is read: `## version — date` (or
 * `## Unreleased`, the next version's notes before the release stamps it), `### Section`, and
 * `- item` lines (an indented line continues the item above). Anything else, such as the intro, is
 * left out.
 */
export function parseChangelog(text: string): Release[] {
  const releases: Release[] = [];
  for (const line of text.split(/\r?\n/)) {
    const release = line.match(/^##\s+(.+?)\s+[—–-]\s+(.+)$/) ?? line.match(/^##\s+(Unreleased)\s*()$/);
    const section = line.match(/^###\s+(.+)$/);
    const item = line.match(/^[-*]\s+(.+)$/);
    const current = releases[releases.length - 1];
    if (release) releases.push({ version: release[1].replace(/^\[|\]$/g, ""), date: release[2].trim(), sections: [] });
    else if (section && current) current.sections.push({ title: section[1].trim(), items: [] });
    else if (item && current?.sections.length) current.sections[current.sections.length - 1].items.push(item[1].trim());
    else if (/^\s{2,}\S/.test(line) && current?.sections.length) {
      const items = current.sections[current.sections.length - 1].items;
      if (items.length) items[items.length - 1] += ` ${line.trim()}`;
    }
  }
  return releases;
}

export const RELEASES = parseChangelog(raw);
export const LATEST = RELEASES[0]?.version ?? "";
