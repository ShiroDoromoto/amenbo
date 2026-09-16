// git's own patch, drawn as git wrote it.
//
// **What a line means is decided where the diff is made.** Reading it back into anything else here
// would be this side deciding it a second time, so the only thing added is a colour, and the colour
// is read off the character git begins each line with.
//
// **It is written once for the two faces that read a patch** — one commit's file in the history,
// and what the rows picked out in the rail are holding (`./GitHistory`, `./GitDiff`). A patch read
// one way in one of them and another way in the other would be two things to learn about one kind
// of text.

/** git's text, one line per row, each one coloured by what it is. */
export function PatchText({ text }: { text: string }) {
  return (
    <>
      {text.split("\n").map((line, at) => (
        // The line's place in the patch, which is the only thing that tells two identical lines
        // apart — a patch is full of them.
        // eslint-disable-next-line react/no-array-index-key
        <span key={at} className={`patch__line patch__line--${kindOf(line)}`}>{line}{"\n"}</span>
      ))}
    </>
  );
}

/** What one line of a patch is, read off the character git begins it with. */
export function kindOf(line: string): "hunk" | "added" | "removed" | "same" {
  // The heads of the file's own two names begin with the same characters a changed line does, and
  // they are three of them rather than one — so they are told apart before the single characters.
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("+++") || line.startsWith("---")) return "hunk";
  if (line.startsWith("+")) return "added";
  if (line.startsWith("-")) return "removed";
  return "same";
}
