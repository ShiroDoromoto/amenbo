// The Viewer's seam — the read and write side for the device settings screen that draws it
// (`AMB-D-884`).
//
// **It takes no project.** The server stands in one Cloudflare account, the key is one key and the read
// code is one code, so every call here is the device's and answers the same wherever the screen is
// standing.
//
// **Nothing here can read a secret back.** The address, the write token and the encryption key setup
// leaves behind stay in core; `setUp` is the whole of what this side learns about them. The one
// exception is `pairCode`, which *is* the key — it comes here because drawing it for a camera is this
// side's job, and it belongs on a screen and nowhere else.
//
// **The state and the pairing are apart on purpose.** The state is read out of this device's own store
// and is there on first paint; whether a phone may read is a question only the server can answer, so it
// arrives later and the screen draws without waiting for it.
import { invoke } from "./ipc";
import { inTauri } from "./snapshot";
import { invalidateQueries, useQuery } from "./query";
import type {
  ViewerAppDto,
  ViewerCodeDto,
  ViewerPairingDto,
  ViewerRepairedDto,
  ViewerSentDto,
  ViewerStateDto,
  ViewerStoodDto,
} from "../bindings/bindings";

/** What this device holds, without asking the network (generated DTO). */
export type ViewerState = ViewerStateDto;
/** Where the phone's half of this is got (generated DTO). */
export type ViewerApp = ViewerAppDto;
/** Whether a phone may read, as the server answers it (generated DTO). */
export type ViewerPairing = ViewerPairingDto;
/** A freshly drawn read code (generated DTO). */
export type ViewerCode = ViewerCodeDto;
/** What one run of setup stood up (generated DTO). */
export type ViewerStood = ViewerStoodDto;
/** What one turn of carrying did (generated DTO). */
export type ViewerSent = ViewerSentDto;
/** What one press of the repair did (generated DTO). */
export type ViewerRepaired = ViewerRepairedDto;

/** What a browser with no store is answered: a device nothing has been set up on. */
const NOTHING: ViewerState = {
  setUp: false,
  carrying: true,
  waiting: 0,
  serverBuild: 0,
  workerBuild: 0,
  tokenLink: "",
};

const NO_APPS: ViewerApp[] = [];

/** What this device holds. Outside Tauri there is no store to ask, so it answers the empty device. */
export async function fetchViewerState(): Promise<ViewerState> {
  if (!inTauri()) return NOTHING;
  return invoke<ViewerState>("viewer_state");
}

/** That state, for the settings section that draws it. */
export function useViewerState(): { state: ViewerState; loading: boolean; error: unknown } {
  const { data, loading, error } = useQuery<ViewerState>(["viewer-state"], fetchViewerState);
  return { state: data ?? NOTHING, loading, error };
}

/**
 * Whether a phone may read. `null` where no server has been stood up — and where there is no store to
 * ask at all.
 *
 * **It goes over the network**, so it is its own query: the section above it draws on first paint and
 * this fills in behind it.
 */
export async function fetchViewerPairing(): Promise<ViewerPairing | null> {
  if (!inTauri()) return null;
  return invoke<ViewerPairing | null>("viewer_pairing");
}

/** The pairing, for the line that says whether any phone is reading. */
export function useViewerPairing(): {
  pairing: ViewerPairing | null;
  loading: boolean;
  error: unknown;
} {
  const { data, loading, error } = useQuery<ViewerPairing | null>(
    ["viewer-pairing"],
    fetchViewerPairing,
  );
  return { pairing: data ?? null, loading, error };
}

/** Refetch what this device holds — after a setup, a send, a repair or the switch. */
function reloadState(): void {
  invalidateQueries((key) => key[0] === "viewer-state");
}

/** Refetch whether a phone may read — after a code was issued or taken away. */
function reloadPairing(): void {
  invalidateQueries((key) => key[0] === "viewer-pairing");
}

/**
 * Where the app is got, one row per kind of phone. It needs no server and no store, so it answers on a
 * device nobody has set anything up on — which is the device most likely to be asking.
 */
export async function fetchViewerApps(): Promise<ViewerApp[]> {
  if (!inTauri()) return NO_APPS;
  return invoke<ViewerApp[]>("viewer_app");
}

/** Those rows, for the section that draws a code beside each link. */
export function useViewerApps(): ViewerApp[] {
  const { data } = useQuery<ViewerApp[]>(["viewer-apps"], fetchViewerApps);
  return data ?? NO_APPS;
}

/**
 * Stand the server up in the reader's own Cloudflare account, or stand it up again over the one there.
 *
 * `apiToken` is spent on this one call and written down nowhere. What comes back says whether the keys
 * already here were kept or drawn afresh — and a key drawn now opens nothing already on the server, so
 * that answer is the sentence the screen owes the reader about pairing their phone again.
 */
export async function setUpViewer(
  apiToken: string,
  account?: string,
  name?: string,
): Promise<ViewerStood | null> {
  if (!inTauri()) return null;
  const stood = await invoke<ViewerStood>("viewer_setup", {
    apiToken,
    account: account ?? null,
    name: name ?? null,
  });
  reloadState();
  reloadPairing();
  return stood;
}

/**
 * Throw the switch this device carries under. Off, nothing is read out and nothing is placed; what is
 * already queued keeps, so turning it back on carries the backlog rather than losing it.
 */
export async function setViewerCarrying(carrying: boolean): Promise<void> {
  if (!inTauri()) return;
  await invoke<null>("viewer_set_carrying", { carrying });
  reloadState();
}

/**
 * Draw a new read code for a camera. `null` where no server has been stood up.
 *
 * **It replaces whatever code the server was holding**, so the phone that had the one before stops
 * reading. The screen says that before the press, not after it.
 */
export async function issueViewerCode(): Promise<ViewerCode | null> {
  if (!inTauri()) return null;
  const code = await invoke<ViewerCode | null>("viewer_pair_code");
  reloadPairing();
  return code;
}

/**
 * Take the read code away. There is one code, so every phone stops reading at once and there is nothing
 * to name. `false` where the server was holding none, `null` where there is no server.
 */
export async function cutOffViewer(): Promise<boolean | null> {
  if (!inTauri()) return null;
  const cut = await invoke<boolean | null>("viewer_cut_off");
  reloadPairing();
  return cut;
}

/**
 * Carry what has moved, now. A device with no server, one whose switch is off and one whose turn
 * another run is taking all answer that they placed nothing — none of the three is a failure, and
 * `heldBack` says which of the last two it was.
 */
export async function sendToViewer(): Promise<ViewerSent | null> {
  if (!inTauri()) return null;
  const sent = await invoke<ViewerSent>("viewer_send");
  reloadState();
  return sent;
}

/**
 * Put right what the server holds, where it has drifted.
 *
 * **Two presses, deliberately.** `place: false` counts the difference and writes it down; the screen
 * shows the reader that number, and `place: true` is what spends it.
 */
export async function repairViewer(place: boolean): Promise<ViewerRepaired | null> {
  if (!inTauri()) return null;
  const done = await invoke<ViewerRepaired>("viewer_repair", { place });
  reloadState();
  return done;
}
