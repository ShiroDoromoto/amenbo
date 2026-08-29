// The press that sends the opening sentence a pane was left holding.
//
// A pane whose sentence the hand-over could not get through leaves it in the agent's input box, one
// keypress from being sent, looking exactly like a pane that was handed its sentence properly
// (`crate::pty`). What rescues it is a person pressing Enter: that is the one thing the hand-over
// could never find out by looking — whether the pane is showing an input box at all — and a person
// sending a line of their own settles it (`AMB-D-805`).
//
// What is pinned here is which crossing counts. The person's line goes through untouched and the
// sentence follows behind it, so the reading is off what the emulator gave the program, not off a
// key; and everything else that travels the same way — a paste, the emulator's own answers, the text
// an input method settled — is not that press.
import { describe, expect, it } from "vitest";
import { sendsTheSentence, SUBMIT } from "./terminal";

describe("the press that sends a sentence left unsent", () => {
  it("is Enter, and only where the host said one is owed", () => {
    expect(SUBMIT, "what an emulator gives the program for Enter").toBe("\r");
    expect(sendsTheSentence(true, SUBMIT)).toBe(true);
  });

  it("is nothing at all in a pane that was never left holding one", () => {
    // The sentence rode in on the command line, or the hand-over got through: there is nothing in the
    // box, and an Enter here is a person sending their own line and no more.
    expect(sendsTheSentence(false, SUBMIT)).toBe(false);
  });

  it("is not what an input method settled, however it ends", () => {
    // A composition produces the text it composed. The Enter that settles it is the method's, and
    // never reaches the program as a press of its own.
    expect(sendsTheSentence(true, "こんにちは")).toBe(false);
    expect(sendsTheSentence(true, "改行\r")).toBe(false);
  });

  it("is not a paste, however many lines are in it", () => {
    // A bracketed paste crosses whole, so a carriage return inside one is text and not a press.
    expect(sendsTheSentence(true, `\x1b[200~one\rtwo\x1b[201~`)).toBe(false);
  });

  it("is not the emulator answering the program", () => {
    // What comes back for a cursor query or a device attribute travels this way too, and none of it
    // is a lone carriage return.
    expect(sendsTheSentence(true, "\x1b[1;1R")).toBe(false);
    expect(sendsTheSentence(true, "\x1b[?1;2c")).toBe(false);
  });

  it("is not Shift-Enter, which the pane answers for itself", () => {
    expect(sendsTheSentence(true, "\x1b\r")).toBe(false);
  });
});
