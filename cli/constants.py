import re

DEFAULT_OUTPUT_DIR = "dl"
URL_PATTERN = re.compile(r"^https?://", re.IGNORECASE)
VIDEO_VARIANT_RE = re.compile(r"((?:media|video)_)(\\d+)(_.*\\.m3u8)", re.IGNORECASE)
COMMON_VIDEO_HEIGHTS = [4320, 2160, 1440, 1080, 720, 540, 480, 360, 240]
