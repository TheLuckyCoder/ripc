from enum import Enum


class OpenMode(Enum):
    ReadOnly = 0,
    WriteOnly = 1,
    ReadWrite = 2


class SharedMemory(object):

    @staticmethod
    def create(name: str, size: int, mode: OpenMode = OpenMode.ReadWrite) -> 'SharedMemory':
        """
        :param name: is recommended to start with a '/'
        :param size: cannot be 0
        :param mode: 
        """
        pass

    @staticmethod
    def open(name: str, mode: OpenMode = OpenMode.ReadWrite) -> 'SharedMemory':
        """
        :param name: is recommended to start with a '/'
        :param mode: 
        """
        pass

    def write(self, bytes_to_write: bytes) -> None:
        """
        Writes the bytes into the shared memory
        
        Will throw an error if the SharedMemory was opened as read-only
        """
        pass

    def try_read(self) -> bytes | None:
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

    def is_new_version_available(self) -> bool:
        """
        Check if the next read will return a new message
        :returns: true if there is a new version
        """
        pass

    def last_written_version(self) -> int:
        """
        :returns: the latest version that was written
        """
        pass

    def last_read_version(self) -> int:
        """
        :returns: the latest version that was read
        """
        pass

    def name(self) -> str:
        """
        :returns: the name of this shared memory file
        """
        pass

    def memory_size(self) -> int:
        """
        :returns: Amount of bytes allocated in this shared memory
        """
        pass

    def is_closed(self) -> bool:
        """
        Check if the shared memory has been closed by the writer
        :returns: true if the writer has marked this as closed
        """

    def close(self) -> None:
        """
        Signals to the readers that they should stop reading from the shared memory
        """
        pass


class SharedMemoryCircularQueue:
    @staticmethod
    def create(name: str, max_element_size: int, capacity: int,
               mode: OpenMode = OpenMode.ReadWrite) -> 'SharedMemoryCircularQueue':
        pass

    @staticmethod
    def open(name: str, mode: OpenMode = OpenMode.ReadWrite) -> 'SharedMemoryCircularQueue':
        pass

    def __len__(self) -> int:
        pass

    def is_full(self) -> bool:
        """
        Checks if the queue is full
        """
        pass

    def try_read(self) -> bytes | None:
        """
        :returns: an element or None, if the queue is empty or closed
        """
        pass

    def blocking_read(self) -> bytes | None:
        """
        Blocks the current thread until an element is available or None if the queue is closed
        :return: an element from the queue
        """
        pass

    def try_write(self, data: bytes) -> bool:
        """
        Tries to write an element to the queue, fails if it's full
        
        :param data: element to add to queue
        :return: true if the operation was successful
        """
        pass

    def blocking_write(self, data: bytes) -> bool:
        """
        Blocks the current thread until the element is added to the queue
        
        :param data: element to add to queue
        :return: false if the queue is closed
        """
        pass

    def read_all(self) -> list[bytes]:
        """
        :returns: a list of all the elements in the queue
        """
        pass

    def name(self) -> str:
        """
        :returns: the name of this shared memory file
        """
        pass

    def memory_size(self) -> int:
        """
        :returns: Amount of bytes allocated in this shared memory
        """
        pass

    def is_closed(self) -> bool:
        """
        Check if the shared memory has been closed by the writer
        :returns: true if the writer has marked this as closed
        """
        pass

    def close(self) -> None:
        """
        Signals to the readers that they should stop reading from the shared memory
        """
        pass


class SharedMemoryQueue:
    @staticmethod
    def create(name: str, max_element_size: int, mode: OpenMode = OpenMode.ReadWrite,
               buffer_size=8) -> 'SharedMemoryQueue':
        pass

    @staticmethod
    def open(name: str, mode: OpenMode = OpenMode.ReadWrite) -> 'SharedMemoryQueue':
        pass

    def write(self, data: bytes):
        """
        Writes an element to the queue
        when called for the first time it creates a feeder thread that will write the data to the shared memory
        This method will never block the current thread
        
        :param data: element to add to queue
        """
        pass

    def try_read(self) -> bytes | None:
        """
        Return an element from the queue if one is available
        :return: an element from the queue or None if the queue is closed
        """
        pass

    def blocking_read(self) -> bytes | None:
        """
        Blocks the current thread until an element is available or None if the queue is closed
        :return: an element from the queue
        """
        pass

    def name(self) -> str:
        """
        :returns: the name of this shared memory file
        """
        pass

    def memory_size(self) -> int:
        """
        :returns: Amount of bytes allocated in this shared memory
        """
        pass

    def is_closed(self) -> bool:
        """
        :returns: true the queue has been marked as closed
        """
        pass

    def close(self) -> None:
        """
        Signals to the readers that they should stop reading and stops all feeder threads
        """
        pass
