from dataclasses import dataclass
from datetime import datetime
from typing import Optional


@dataclass
class LinkEntry:
    url: str
    course: str


@dataclass
class StreamInfo:
    title: str
    audio_m3u8: str
    video_m3u8: str
    pub_date: Optional[datetime] = None


class CliError(Exception):
    pass
