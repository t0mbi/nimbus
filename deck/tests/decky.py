"""Minimal stand-in for Decky Loader's `decky` module, so main.py can be
imported and its pure logic exercised in a normal Python environment,
outside the actual Decky runtime. Test-only - not part of the packaged
plugin (see the packaging step, which only ships main.py/py_modules/dist).
"""
import logging

logger = logging.getLogger("decky-stub")
DECKY_PLUGIN_SETTINGS_DIR = "/tmp/nimbus-test-settings"
DECKY_PLUGIN_DIR = "/tmp/nimbus-test-plugin"


async def emit(event, *args):
    print(f"[emit] {event} {args}")
