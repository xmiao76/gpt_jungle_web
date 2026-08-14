"""Browser-only search enhancements layered over the manifest-locked engine."""

from .search import BrowserHardAI, BrowserCompactSearch

HARD_AI_VERSION = "web-hard-v2"

__all__ = ["BrowserHardAI", "BrowserCompactSearch", "HARD_AI_VERSION"]
