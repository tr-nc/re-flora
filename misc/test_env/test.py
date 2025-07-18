import sys
from pathlib import Path

# force a non-GUI backend for environments without display servers
import matplotlib
matplotlib.use("Agg")  # noqa: E402

import matplotlib.pyplot as plt
import cv2
import numpy as np


def linear_scale_to_u8(hdr: np.ndarray) -> np.ndarray:
    """
    linearly map the full dynamic range of an HDR image to 0-255 uint8.
    """
    min_val = float(np.min(hdr))
    max_val = float(np.max(hdr))

    # avoid division by zero if the image is constant
    if np.isclose(max_val, min_val):
        return np.zeros_like(hdr, dtype=np.uint8)

    scaled = (hdr - min_val) / (max_val - min_val)
    return np.clip(np.round(scaled * 255.0), 0, 255).astype(np.uint8)


def process_hdr(
    input_path: str,
    output_image_path: str,
    hist_path: str,
) -> None:
    """
    read an HDR image, print statistics, plot histogram, apply linear
    scaling to full 8-bit dynamic range, clip the result and save.
    """
    hdr = cv2.imread(input_path, cv2.IMREAD_ANYDEPTH | cv2.IMREAD_COLOR)
    if hdr is None:
        raise ValueError(f"failed to read input image: {input_path}")

    print_stats(hdr)
    save_histogram(hdr, hist_path)

    u8_image = linear_scale_to_u8(hdr)

    if not cv2.imwrite(output_image_path, u8_image):
        raise IOError(f"failed to write output image: {output_image_path}")

    print(f"8-bit image saved to {output_image_path}")
    print(f"histogram saved to {hist_path}")


def print_stats(img: np.ndarray) -> None:
    """
    compute and print global and per-channel statistics
    (min, max, mean, median).
    """
    flat = img.reshape(-1, img.shape[-1]) if img.ndim == 3 else img.reshape(-1, 1)
    channels = flat.shape[1]

    overall_min = float(np.min(flat))
    overall_max = float(np.max(flat))
    overall_mean = float(np.mean(flat))
    overall_median = float(np.median(flat))

    print("overall:")
    print(f"  min={overall_min:.6f}  max={overall_max:.6f}")
    print(f"  mean={overall_mean:.6f} median={overall_median:.6f}")

    if channels > 1:
        names = ("R", "G", "B", "A")
        for idx in range(channels):
            c = flat[:, idx]
            c_min = float(np.min(c))
            c_max = float(np.max(c))
            c_mean = float(np.mean(c))
            c_median = float(np.median(c))
            print(f"{names[idx] if idx < len(names) else f'C{idx}'}:")
            print(f"  min={c_min:.6f}  max={c_max:.6f}")
            print(f"  mean={c_mean:.6f} median={c_median:.6f}")


def save_histogram(img: np.ndarray, out_path: str) -> None:
    """
    plot and save per-channel histogram with 256 bins.
    """
    if img.ndim == 2:
        img = img[..., np.newaxis]

    colors = ("r", "g", "b", "m")
    channel_names = ("R", "G", "B", "A")

    plt.figure(figsize=(8, 4))
    for idx in range(img.shape[-1]):
        data = img[..., idx].flatten()
        plt.hist(
            data,
            bins=256,
            color=colors[idx % len(colors)],
            alpha=0.5,
            label=channel_names[idx] if idx < len(channel_names) else f"C{idx}",
            histtype="stepfilled",
            edgecolor="black",
        )

    plt.title("HDR pixel value distribution")
    plt.xlabel("value")
    plt.ylabel("frequency")
    plt.legend()
    plt.tight_layout()
    plt.savefig(out_path, dpi=150)
    plt.close()


def main() -> None:
    """
    usage:
        python process_hdr.py [input_hdr] [output_png] [hist_png]

    defaults:
        input_hdr  = ./out.hdr
        output_png = ./out_u8.png
        hist_png   = ./out_hist.png
    """
    args = sys.argv[1:]

    input_path = args[0] if len(args) > 0 else "./out.hdr"
    output_path = args[1] if len(args) > 1 else "./out_u8.png"
    hist_path = args[2] if len(args) > 2 else "./out_hist.png"

    if not Path(input_path).is_file():
        raise FileNotFoundError(f"input file does not exist: {input_path}")

    process_hdr(input_path, output_path, hist_path)


if __name__ == "__main__":
    main()