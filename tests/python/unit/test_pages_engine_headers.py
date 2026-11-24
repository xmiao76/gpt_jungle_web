from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def test_static_pages_headers_support_wasm_release_and_isolation() -> None:
    headers = (ROOT / "public" / "_headers").read_text(encoding="utf-8")

    assert "Cross-Origin-Opener-Policy: same-origin" in headers
    assert "Cross-Origin-Embedder-Policy: require-corp" in headers
    assert "Cross-Origin-Resource-Policy: same-origin" in headers
    assert "Content-Type: application/wasm" in headers
    assert "/conformance/metadata.json" in headers
