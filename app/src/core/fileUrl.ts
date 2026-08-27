// Stream URLs for the files a project's folders hold.
//
// A custom protocol on the Rust side (`amenbofile`, app/src-tauri/src/fileproto.rs) hands the
// webview one file **by its path**, fenced to the folders the project is bound to. A viewer puts
// the URL built here in the `src` of an `<img>`, so the picture streams in and draws top to bottom
// instead of arriving whole as a base64 `data:` URL (`AMB-D-783`).
//
// The address is a project, one of its bound folders, and then a path under that folder:
//
//     amenbofile://localhost/<project>/<root>/<segment>/<segment>…?mime=<type>
//
// **Every part is encoded on its own.** The host splits the path on `/` before it decodes anything,
// so a separator inside a name — or inside the folder's own absolute path — has to arrive encoded
// or it would read as a segment boundary. That is also the fence's own rule: a segment that decodes
// into a path is refused rather than resolved (`crate::fileproto`).
import { schemeBase } from "./customScheme";

const SCHEME = "amenbofile";

/**
 * Builds the stream URL for a file under one of a project's bound folders.
 *
 * `root` is the folder as the store spells it and `path` the segments under it — the same three
 * things `folderRead` is called with, so a caller that has an answer already has its address.
 *
 * `mime` rides along in the query string as the `Content-Type`: the bytes were sniffed on the host
 * when the file was read, and the door serves what it is told rather than guessing from the name.
 */
export function fileUrl(
  projectId: number,
  root: string,
  path: string[],
  mime: string | null | undefined,
): string {
  const segments = path.map(encodeURIComponent).join("/");
  const q = mime ? `?mime=${encodeURIComponent(mime)}` : "";
  return `${schemeBase(SCHEME)}/${projectId}/${encodeURIComponent(root)}/${segments}${q}`;
}
