"""Runtime helpers — signal handlers, main-thread guard."""

from __future__ import annotations

import asyncio
import logging
import signal
import threading
from collections.abc import Callable

_log = logging.getLogger("atd_server")


def install_signal_handlers(
    loop: asyncio.AbstractEventLoop,
    on_stop: Callable[[], None],
) -> None:
    """Install SIGTERM / SIGINT → on_stop on the given loop.

    No-op on platforms / threads that do not support signal handlers
    (Windows; non-main threads). Caller still controls shutdown via
    `await server.stop()` in those cases.
    """
    if threading.current_thread() is not threading.main_thread():
        _log.debug("skipping signal handlers: not main thread")
        return
    for sig in (signal.SIGTERM, signal.SIGINT):
        try:
            loop.add_signal_handler(sig, on_stop)
        except (NotImplementedError, RuntimeError) as e:
            _log.debug("skipping signal %s: %s", sig.name, e)
