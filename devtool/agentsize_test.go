package main

import (
	"strings"
	"testing"
)

// TestSizesFromJSONSplitsTheDocument pins that the section sizes come from splitting the
// real document rather than from re-adding numbers the tool computed itself — the signal
// exists to attribute growth to a section, and an attribution that drifts from the
// document is worse than none. It also pins the total against the section sum: the total
// covers the keys and punctuation no section owns, so it must exceed that sum, and a
// total defined as the sum would hide exactly the growth that comes from adding sections.
func TestSizesFromJSONSplitsTheDocument(t *testing.T) {
	doc := []byte(`{"notes":["ab","cd"],"conventions":{"id":"x"},"version":"1.0.0"}`)
	m, err := sizesFromJSON(doc)
	if err != nil {
		t.Fatal(err)
	}
	if got, want := m.Sections["notes"], len(`["ab","cd"]`); got != want {
		t.Fatalf("notes: got %d, want %d", got, want)
	}
	if got, want := m.Sections["conventions"], len(`{"id":"x"}`); got != want {
		t.Fatalf("conventions: got %d, want %d", got, want)
	}
	if m.Total != len(doc) {
		t.Fatalf("total: got %d, want %d", m.Total, len(doc))
	}
	sum := 0
	for _, v := range m.Sections {
		sum += v
	}
	if m.Total <= sum {
		t.Fatalf("total %d should exceed the section sum %d", m.Total, sum)
	}
}

func TestSizesFromJSONRejectsNonObject(t *testing.T) {
	if _, err := sizesFromJSON([]byte(`["not","an","object"]`)); err == nil {
		t.Fatal("a non-object document must be an error, not an empty measurement")
	}
}

// TestDeltaRowsKeepsOneSidedSections pins that a section the diff adds or removes, which
// exists on one side only, still gets a row — those are the two cases the reader most
// needs, so neither may be dropped for want of a counterpart.
func TestDeltaRowsKeepsOneSidedSections(t *testing.T) {
	base := measurement{Sections: sizes{"notes": 100, "gone": 50}}
	head := measurement{Sections: sizes{"notes": 100, "added": 30}}
	rows := deltaRows(base, head)
	got := map[string]row{}
	for _, r := range rows {
		got[r.Section] = r
	}
	if len(got) != 3 {
		t.Fatalf("want notes/gone/added, got %d rows", len(got))
	}
	if got["added"].Base != 0 || got["added"].delta() != 30 {
		t.Fatalf("a section only head has must read as growth from zero: %+v", got["added"])
	}
	if got["gone"].Head != 0 || got["gone"].delta() != -50 {
		t.Fatalf("a section only base had must read as a drop to zero: %+v", got["gone"])
	}
}

// TestDeltaRowsOrdersByGrowth pins biggest growth first — the ordering is what lets the
// author see what their diff put in the entry without reading every row.
func TestDeltaRowsOrdersByGrowth(t *testing.T) {
	base := measurement{Sections: sizes{"a": 10, "b": 10, "c": 10}}
	head := measurement{Sections: sizes{"a": 12, "b": 40, "c": 5}}
	rows := deltaRows(base, head)
	if rows[0].Section != "b" || rows[1].Section != "a" || rows[2].Section != "c" {
		t.Fatalf("want b, a, c by delta; got %s, %s, %s", rows[0].Section, rows[1].Section, rows[2].Section)
	}
}

// TestRenderAsksTheQuestionOnGrowth pins that growth puts the question to the author
// (spec or argument?), since that handoff is the whole mechanism and without it this is a
// number nobody acts on, and that a shrink is not something to interrogate.
func TestRenderAsksTheQuestionOnGrowth(t *testing.T) {
	base := measurement{Sections: sizes{"notes": 100}, Total: 1000}
	head := measurement{Sections: sizes{"notes": 1500}, Total: 2400}
	out := render(base, head, "main", "abcdef1234567890")
	if !strings.Contains(out, "+1,400") {
		t.Fatalf("the total delta must be shown: %s", out)
	}
	if !strings.Contains(out, "spec, or an argument") {
		t.Fatalf("growth must hand the judgement to the author: %s", out)
	}
	if !strings.Contains(out, "abcdef123456") {
		t.Fatalf("the base commit must be named so the comparison is reproducible: %s", out)
	}
	shrunk := render(head, base, "main", "abcdef1234567890")
	if strings.Contains(shrunk, "spec, or an argument") {
		t.Fatalf("shrinking must not be questioned: %s", shrunk)
	}
	if !strings.Contains(shrunk, "-1,400") {
		t.Fatalf("a shrink must read as negative: %s", shrunk)
	}
}

func TestComma(t *testing.T) {
	for in, want := range map[int]string{0: "0", 999: "999", 1000: "1,000", 45930: "45,930", -1822: "-1,822"} {
		if got := comma(in); got != want {
			t.Fatalf("comma(%d): got %s, want %s", in, got, want)
		}
	}
}
