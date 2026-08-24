#!/usr/bin/env python3
# filemanager1.py — the far side of "show this in the folder".
#
# That door does not start a program. It asks the session bus for whoever owns
# org.freedesktop.FileManager1 and hands the item over, so with no such owner on
# the bus the call fails and nothing at all appears on screen — which is exactly
# what an empty container looks like when the feature works.
#
# So this claims the name. It is installed as a bus-activatable service, not
# started by hand: the bus launches it when the first call arrives, the same way
# a desktop's own file manager is launched, and a call that never left the app
# therefore leaves no trace of it.
#
# What it does with the call is record it and put a window on the display. The
# record is the evidence (the log line names the method and the URIs); the window
# is what a screenshot of the road can show.

import sys
import time

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import Gtk  # noqa: E402  (the version has to be pinned first)

BUS_NAME = "org.freedesktop.FileManager1"
OBJECT_PATH = "/org/freedesktop/FileManager1"
LOG = "/out/opened.log"


def record(method, uris):
    stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    line = "%s\tfilemanager1.%s\t%s\n" % (stamp, method, " ".join(uris))
    with open(LOG, "a") as fh:
        fh.write(line)
    sys.stdout.write(line)
    sys.stdout.flush()


class FileManager1(dbus.service.Object):
    def __init__(self, bus_name):
        dbus.service.Object.__init__(self, bus_name, OBJECT_PATH)
        self.window = None

    @dbus.service.method(BUS_NAME, in_signature="ass", out_signature="")
    def ShowItems(self, uris, startup_id):
        self.announce("ShowItems", uris)

    @dbus.service.method(BUS_NAME, in_signature="ass", out_signature="")
    def ShowFolders(self, uris, startup_id):
        self.announce("ShowFolders", uris)

    @dbus.service.method(BUS_NAME, in_signature="ass", out_signature="")
    def ShowItemProperties(self, uris, startup_id):
        self.announce("ShowItemProperties", uris)

    # One window, kept in the corner. There is no window manager on this display, so a
    # window cannot be moved or closed by hand: left to itself a second call would
    # stack another sheet over the app and there would be no way to lift it off.
    def announce(self, method, uris):
        shown = [str(u) for u in uris]
        record(method, shown)
        if self.window is not None:
            self.window.destroy()
        self.window = Gtk.Window(title="Fake File Manager")
        self.window.set_default_size(560, 90)
        self.window.move(20, 780)
        self.window.add(Gtk.Label(label="%s\n%s" % (method, "\n".join(shown))))
        self.window.show_all()


DBusGMainLoop(set_as_default=True)
FileManager1(dbus.service.BusName(BUS_NAME, dbus.SessionBus()))
Gtk.main()
