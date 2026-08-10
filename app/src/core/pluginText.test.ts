// The one rule these three functions hold (`AMB-D-623`): the reader's language when the author wrote it,
// the author's own when they did not. Pinned here rather than at each face, because the four faces that
// draw a plugin's words must not each answer it differently — an untranslated label beside a translated
// one is the ordinary state, and it has to read as one line of prose either way.
import { describe, expect, it } from "vitest";
import type { PluginConfigOptionDto, PluginWantedSettingDto } from "../bindings/bindings";
import { optionLabel, pluginDesc, settingLabel } from "./pluginText";

const setting = (over: Partial<PluginWantedSettingDto>): PluginWantedSettingDto => ({
  key: "channel",
  label: "Channel",
  secret: false,
  required: true,
  fieldType: "text",
  options: [],
  ...over,
});

const option = (over: Partial<PluginConfigOptionDto>): PluginConfigOptionDto => ({
  value: "task.done",
  label: "Task finished",
  ...over,
});

describe("pluginDesc", () => {
  it("draws the reader's language where the catalog published one", () => {
    expect(pluginDesc({ desc: "post to a channel", descI18n: "チャンネルに投稿する" }))
      .toBe("チャンネルに投稿する");
  });

  // Absent is every ordinary case at once — nobody translated it, nobody published that language, or the
  // reader reads the base language — and none of them is worth a different answer.
  it("falls back to the author's own line, and treats null as absent", () => {
    expect(pluginDesc({ desc: "post to a channel" })).toBe("post to a channel");
    expect(pluginDesc({ desc: "post to a channel", descI18n: null })).toBe("post to a channel");
  });
});

describe("settingLabel", () => {
  it("captions a field in the reader's language, else the author's", () => {
    expect(settingLabel(setting({ labelI18n: "チャンネル" }))).toBe("チャンネル");
    expect(settingLabel(setting({}))).toBe("Channel");
  });
});

describe("optionLabel", () => {
  it("captions a candidate in the reader's language, else the author's", () => {
    expect(optionLabel(option({ labelI18n: "タスクが終わったとき" }))).toBe("タスクが終わったとき");
    expect(optionLabel(option({}))).toBe("Task finished");
  });

  // The value is the plugin's wire vocabulary, so a translation never has one to offer for it.
  it("leaves the stored value alone", () => {
    const o = option({ labelI18n: "タスクが終わったとき" });
    expect(o.value).toBe("task.done");
  });
});
