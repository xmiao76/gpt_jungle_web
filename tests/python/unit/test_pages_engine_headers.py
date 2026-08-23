from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def test_static_pages_headers_enable_threaded_wasm_isolation() -> None:
    headers = (ROOT / "public" / "_headers").read_text(encoding="utf-8")

    assert "Cross-Origin-Opener-Policy: same-origin" in headers
    assert "Cross-Origin-Embedder-Policy: require-corp" in headers
    assert "Cross-Origin-Resource-Policy: same-origin" in headers
    assert "/generated/jungle-wasm/*" in headers
    assert "/generated/tablebase/*" in headers
    assert "/generated/engine-runtime-manifest.json" in headers
