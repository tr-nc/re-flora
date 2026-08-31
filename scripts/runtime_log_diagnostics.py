#!/usr/bin/env python3
"""Classify fatal diagnostics shared by runtime evidence and smoke gates."""

from __future__ import annotations

import re


LOG_TIME = r"(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d\.\d{3}"

FATAL_DIAGNOSTIC = re.compile(
    r"\b(?:VK_)?ERROR(?:_[A-Z0-9]+)+\b|\bERROR\b|"
    r"\bvalidation\s+(?:error|failure)\b|\bpanic(?:ked)?\b|"
    r"VUID-|\bdevice\s+lost\b|\bdestroyed\s+descriptor\b|"
    r"\bstale\s+readback\b",
    re.IGNORECASE | re.MULTILINE,
)
BENIGN_PLATFORM_DIAGNOSTIC = re.compile(
    rf"^\[{LOG_TIME} ERROR sctk_adwaita::config\] "
    r"XDG Settings Portal did not return response in time: "
    r"timeout: 100ms, key: color-scheme$",
    re.MULTILINE,
)
PORTAL_TIMEOUT_DIAGNOSTIC = re.compile(
    r"XDG\s+Settings\s+Portal\s+did\s+not\s+return\s+response\s+in\s+time",
    re.IGNORECASE,
)


def _mask_benign_platform_diagnostics(text: str) -> str:
    return BENIGN_PLATFORM_DIAGNOSTIC.sub(
        lambda match: " " * len(match.group(0)),
        text,
    )


def fatal_diagnostics(text: str) -> list[re.Match[str]]:
    """Return ordered fatal matches after removing only exact owned exceptions."""
    classified = _mask_benign_platform_diagnostics(text)
    matches = [
        *PORTAL_TIMEOUT_DIAGNOSTIC.finditer(classified),
        *FATAL_DIAGNOSTIC.finditer(classified),
    ]
    matches.sort(key=lambda match: (match.start(), -match.end()))
    return matches


def first_fatal_diagnostic(text: str) -> re.Match[str] | None:
    matches = fatal_diagnostics(text)
    return matches[0] if matches else None


def fatal_diagnostic_excerpts(text: str) -> list[str]:
    """Return unique complete physical lines containing classified diagnostics."""
    excerpts: list[str] = []
    for diagnostic in fatal_diagnostics(text):
        start = text.rfind("\n", 0, diagnostic.start()) + 1
        end = text.find("\n", diagnostic.end())
        if end < 0:
            end = len(text)
        excerpt = text[start:end]
        if excerpt not in excerpts:
            excerpts.append(excerpt)
    return excerpts
