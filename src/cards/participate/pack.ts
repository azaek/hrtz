// A report packed small enough to ride in a GitHub issue's text, so the issue alone holds what
// the file does: deflated, then written in the 64 characters a link and Markdown both leave alone.
// A mouse's 6,912 bytes of memory are mostly blank, and pack to a couple of hundred characters.
// `scripts/report.mjs` reads one back.

/** What a packed report starts with in an issue's text. */
export const PACKED = "hrtz-report/2:";

async function through(bytes: Uint8Array<ArrayBuffer>, stream: CompressionStream | DecompressionStream): Promise<Uint8Array> {
  return new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(stream)).arrayBuffer());
}

function toText(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromText(text: string): Uint8Array<ArrayBuffer> {
  const binary = atob(text.replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** `value` as text for an issue. */
export async function pack(value: unknown): Promise<string> {
  return toText(await through(new TextEncoder().encode(JSON.stringify(value)), new CompressionStream("deflate-raw")));
}

/** What `pack` was given. */
export async function unpack(text: string): Promise<unknown> {
  return JSON.parse(new TextDecoder().decode(await through(fromText(text), new DecompressionStream("deflate-raw"))));
}
