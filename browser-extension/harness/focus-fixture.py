"""A text field in a second application on the private X11 display."""
import signal
import sys
from pathlib import Path

import gi

gi.require_version('Gtk', '3.0')
from gi.repository import GLib, Gtk

output = Path(sys.argv[1])
window = Gtk.Window(title='Synthetic Mediacenter focus fixture')
window.set_default_size(380, 100)
entry = Gtk.Entry()
entry.connect('changed', lambda field: output.write_text(field.get_text(), encoding='utf-8'))
entry.connect('activate', lambda _: output.with_suffix('.enter').touch())
window.add(entry)
window.connect('destroy', Gtk.main_quit)
window.show_all()
entry.grab_focus()
output.write_text('', encoding='utf-8')
GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM, Gtk.main_quit)
Gtk.main()
