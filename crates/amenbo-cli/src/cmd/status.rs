//! `status`, and the no-argument discover: what to do now — what is overdue, what is due
//! today, and what to take next.

use amenbo_core::{query, time};

use crate::cmd::labels::task_label;

pub(crate) fn render_status(s: &query::StatusResult) {
    println!("== {} ==", time::date_to_string(s.today_date));
    println!("overdue {} / today {} / in progress {} / within 7 days {} / no due date {} / completed today {}",
        s.counts.overdue, s.counts.due_today, s.counts.in_progress, s.counts.upcoming_7d, s.counts.no_due, s.counts.completed_today);
    if !s.overdue.is_empty() {
        println!("[Overdue]");
        for o in &s.overdue {
            println!("  {}  {} ({} day(s) overdue)", task_label(o.task.id), o.task.title, o.days_overdue);
        }
    }
    if let Some(dt) = &s.due_today {
        if !dt.is_empty() {
            println!("[Due today]");
            for t in dt {
                println!("  {}  {}", task_label(t.id), t.title);
            }
        }
    }
    // Unlike Overdue and Due today, whose counts appear on the summary line, suggestions are counted nowhere.
    // So always print the section, and say `(none)` when there is nothing to suggest — printing nothing would
    // be indistinguishable from the feature not existing.
    println!("[Next suggestions]");
    if s.next_suggested.is_empty() {
        println!("  (none)");
    } else {
        for n in &s.next_suggested {
            println!("  {}  {} — {}", task_label(n.id), n.title, n.reason);
        }
    }
}

pub(crate) fn render_discover(d: &query::DiscoverResult) {
    println!("== {} ==", time::date_to_string(d.today_date));
    if d.today.is_empty() {
        println!("No tasks for today.");
    } else {
        println!("[Today]");
        for t in &d.today {
            let check = if t.completed { "x" } else { " " };
            println!("  [{check}] {}  {}", task_label(t.id), t.title);
        }
    }
    for h in &d.hints {
        println!("- {h}");
    }
}
