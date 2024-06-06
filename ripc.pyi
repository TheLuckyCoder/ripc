class SharedMemoryWriter(object):
    def __init__(self, name: str, size: int):
        """
        :parameter name is recommended to start with a '/'
        :parameter size cannot be 0
        """
        pass

    def write(self, bytes_to_write: bytes) -> None:
        """
        Reads the shared memory into a buffer then allocates a bytes object
        """
        pass

    def name(self) -> str:
        """
        :returns: the name of this shared memory file
        """
        pass

    def size(self) -> int:
        """
        :returns: Amount of bytes allocated in this shared memory
        """
        pass

class SharedMemoryReader:
    def __init__(self, name: str): ...

    def read(self) -> bytes | None:
        """
        Reads the shared memory into a buffer then allocates a bytes object
        """
        pass

    def read_in_place(self, ignore_same_version: bool) -> bytes | None:
        """
        Allocates an bytes object directly without reading to an intermediate buffer
        Can be faster but might hold the read lock for longer
        """
        pass

    def name(self) -> str:
        """
        :returns: the name of this shared memory file
        """
        pass

    def size(self) -> int:
        """
        :returns: Amount of bytes allocated in this shared memory
        """
        pass

class V4lSharedMemoryWriter:
    def __init__(self, device_path: str, video_width: int, video_height: int, memory_topic: str):...

    # Stops the video transmission, the object cannot be used afterwards
    def stop(self) -> None: ...

class SharedMemoryCircularQueue:
    def __init__(self, device_path: str, create: bool=False, element_size:int=0, elements_count:int=0):
        pass
    
    def __len__(self) -> int:
        pass

    def is_full(self) -> bool:
        pass
    
    def try_read(self) -> bytes | None:
        pass
    
    def blocking_read(self) -> bytes | None:
        pass
    
    def try_write(self, data: bytes) -> bool:
        pass
    
    def blocking_write(self, data: bytes):
        pass
