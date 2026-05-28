"""Minimal mnemos REST client for Hermes Agent integration."""
from __future__ import annotations
import json
import os
import urllib.parse
import urllib.request
from typing import Any, Iterable, Optional

DEFAULT_URL = os.environ.get("MNEMOS_URL", "http://localhost:7423")
DEFAULT_TOKEN = os.environ.get("MNEMOS_TOKEN", "")


def _req(method: str, path: str, body: Optional[dict] = None) -> Any:
    url = f"{DEFAULT_URL}{path}"
    data = json.dumps(body).encode("utf-8") if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("authorization", f"Bearer {DEFAULT_TOKEN}")
    if body is not None:
        req.add_header("content-type", "application/json")
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode("utf-8"))


def remember(
    body: str,
    *,
    title: Optional[str] = None,
    tier: str = "semantic",
    tags: Optional[Iterable[str]] = None,
    importance: Optional[float] = None,
) -> str:
    payload: dict = {"body": body, "tier": tier}
    if title is not None:
        payload["title"] = title
    if tags is not None:
        payload["tags"] = list(tags)
    if importance is not None:
        payload["importance"] = float(importance)
    return _req("POST", "/v1/memories", payload)["id"]


def recall(query: str, *, k: int = 10, explain: bool = False, graph: bool = True) -> list[dict]:
    return _req(
        "POST",
        "/v1/memories/search",
        {"query": query, "k": k, "explain": explain, "graph": graph},
    )["hits"]


def forget(memory_id: str, reason: Optional[str] = None) -> dict:
    path = f"/v1/memories/{memory_id}"
    if reason:
        path += f"?reason={urllib.parse.quote(reason)}"
    return _req("DELETE", path)


def get_memory(memory_id: str) -> dict:
    return _req("GET", f"/v1/memories/{memory_id}")


def list_memories(*, tier: Optional[Iterable[str]] = None, limit: int = 50) -> list[dict]:
    q = []
    if tier:
        for t in tier:
            q.append(f"tier={t}")
    q.append(f"limit={limit}")
    return _req("GET", f"/v1/memories?{'&'.join(q)}")["memories"]


if __name__ == "__main__":
    import sys
    if len(sys.argv) >= 3 and sys.argv[1] == "remember":
        print(remember(" ".join(sys.argv[2:])))
    elif len(sys.argv) >= 3 and sys.argv[1] == "recall":
        for h in recall(" ".join(sys.argv[2:])):
            print(f"{h['memory']['id']}\t{h['memory']['title']}")
    else:
        print("usage: hermes_mnemos.py [remember|recall] <text>")
