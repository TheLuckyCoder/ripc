class SharedMemoryWriter(object):
    def __init__(self, name: str, size: int):
        """
        :parameter name is recommended to start with a '/'
        :parameter size cannot be 0
        """
        pass

    def write(self, bytes_to_write: bytes) -> None:
        """
        Writes the bytes into the shared memory
        """
        pass

    def name(self) -> str:
        """
        :returns: the name of this shared memory file
        """
        pass

    def total_allocated_size(self) -> int:
        pass

    def size(self) -> int:
        """
        :returns: Amount of bytes allocated in this shared memory
        """
        pass

    def last_written_version(self):
        pass

    def close(self) -> None:
        """
        Signals to the readers that they should stop reading from the shared memory
        """
        pass


class SharedMemoryReader:
    def __init__(self, name: str): ...

    def try_read(self, ignore_same_version: bool = True) -> bytes | None:
        """
        :returns: the message, or None if it's the same version as the last time or if the shared memory is closed
        """
        pass

    def blocking_read(self) -> bytes | None:
        """
        Keeps checking the shared memory until there is a new version to read,
        This function also releases the GIL, while waiting for a new message
        :returns: the message, or None if the shared memory is closed
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

    def check_message_available(self) -> bool:
        """
        Check if the next read with return a new value
        :return:
        """
        pass

    def last_read_version(self) -> int:
        pass

    def is_closed(self) -> bool:
        """
        Check if the shared memory has been closed by the writer
        """


class SharedMemoryCircularQueue:
    @staticmethod
    def create(name: str, element_size: int, elements_count: int):
        pass
    
    @staticmethod
    def open(name: str):
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
