from dataclasses import dataclass


@dataclass
class LinkEntry:
    url: str
    course: str


@dataclass
class StreamInfo:
    title: str
    audio_m3u8: str
    video_m3u8: str


class CliError(Exception):
    pass
