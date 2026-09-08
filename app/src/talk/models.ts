// What the two faces that offer models agree on (`AMB-D-865`).
//
// One of them chooses what a pane **opens** on (`../shell/EmptySlot`) and the other moves a pane that
// is **already running** (`../shell/PaneModel`), and the road each takes is different: the first puts
// a flag on a launch line, the second types the provider's own command into a program that started an
// hour ago. What they share is the shape of the answer — a row of pills, a box where the row would be
// a page, and a box with nothing but history behind it where the provider will not say — so the
// number that decides which of the three is drawn is written here and read by both.

/** How many models a row draws before it grows a box to narrow itself with.
 *
 *  It is a row of pills, and a row is something an eye takes in at once: Cursor answered with 217,
 *  which is a page. The number is where the six providers actually fall — one answered with eleven
 *  and the rest with six or fewer, so every provider that *can* be read as a row is drawn as one. */
export const MANY = 12;
