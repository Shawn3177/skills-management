"""Render the Skills Manage app icon: a round, softly blended gradient, mostly
blue (#B3C9FF) warming through lilac to peach (#F8AD7C) at the bottom right.

    python src-tauri/icons/make_app_icon.py src-tauri/icons/app-icon.png
    (also refresh src/assets/app-icon-64.png, the "All" tab icon)
    npx tauri icon src-tauri/icons/app-icon.png
"""

import sys

import numpy as np
from PIL import Image

SIZE = 1024
RADIUS = 0.47  # the disc fills almost the whole icon, like other desktop icons

# Soft color fields that blend into each other across the whole disc (a mesh
# gradient), so there is no visible boundary: blue covers most of it, warming
# through lilac blush to peach towards the bottom right.
# (x, y, spread, weight, rgb)
FIELDS = [
    (0.26, 0.26, 0.34, 1.25, (179, 201, 255)),  # #B3C9FF soft blue
    (0.74, 0.20, 0.28, 0.85, (192, 192, 255)),  # periwinkle
    (0.28, 0.76, 0.24, 0.55, (206, 196, 255)),  # lavender
    (0.58, 0.60, 0.22, 0.60, (242, 196, 228)),  # pink blush (keeps the blend clean)
    (0.86, 0.86, 0.26, 1.15, (250, 170, 118)),  # #F8AD7C peach
]


def render(path: str) -> None:
    ys, xs = np.mgrid[0:SIZE, 0:SIZE].astype(np.float64) / SIZE

    rgb = np.zeros((SIZE, SIZE, 3))
    total = np.zeros((SIZE, SIZE))
    for x, y, spread, weight, color in FIELDS:
        w = weight * np.exp(-((xs - x) ** 2 + (ys - y) ** 2) / (2 * spread * spread))
        rgb += np.array(color, dtype=np.float64) * w[..., None]
        total += w
    rgb /= np.maximum(total, 1e-6)[..., None]

    distance = np.sqrt((xs - 0.5) ** 2 + (ys - 0.5) ** 2)
    alpha = np.clip((RADIUS - distance) * SIZE / 1.6 * 0.5 + 0.5, 0, 1)

    image = np.dstack([np.clip(rgb, 0, 255), alpha * 255]).astype(np.uint8)
    Image.fromarray(image, "RGBA").save(path)


if __name__ == "__main__":
    render(sys.argv[1] if len(sys.argv) > 1 else "app-icon.png")
