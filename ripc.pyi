class SharedMemoryWriter(object):
    # Topic is recommended to start with a '/'
    def __init__(self, topic: str, size_mb: int): ...

    # Reads the shared memory into a buffer then allocates a bytes object
    def write(self, bytes_to_write: bytes) -> None: ...

    # Amount of bytes allocated in this shared memory
    def size(self) -> int: ...

class SharedMemoryReader:
    def __init__(self, topic: str): ...

    # Reads the shared memory into a buffer then allocates a bytes object
    def read(self) -> bytes: ...

    # Allocates an bytes object directly without reading to an intermediate buffer
    # Can be faster but might hold the read lock for longer
    def read_in_place(self, ignore_same_version: bool) -> bytes: ...

    # Amount of bytes allocated in this shared memory
    def size(self) -> int: ...

class V4lSharedMemoryWriter:
    def __init__(self, device_path: str, video_width: int, video_height: int, memory_topic: str):...

    # Stops the video transmission, the object cannot be used afterwards
    def stop(self) -> None: ...