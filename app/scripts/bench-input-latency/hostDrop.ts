// Put in place of `src/core/hostDrop.ts`: files dropped from the desktop come from the host's window,
// which there is none of here.
export * from "@app/core/hostDrop";

export async function watchHostDrop(): Promise<() => void> {
  return () => {};
}
