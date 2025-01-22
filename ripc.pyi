from enum import Enum


class OpenMode(Enum):
    ReadOnly = 0,
    WriteOnly = 1,
    ReadWrite = 2


class SharedMessage(object):

    @staticmethod
    def create(name: str, size: int, mode: OpenMode = OpenMode.ReadWrite) -> 'SharedMessage':
        """
        :param name: is recommended to start with a '/'
        :param size: cannot be 0
        :param mode: 
        """
        pass

    @staticmethod
    def open(name: str, mode: OpenMode = OpenMode.ReadWrite) -> 'SharedMessage':
        """
        :param name: is recommended to start with a '/'
        :param mode: 
        """
        pass

    def write(self, data: bytes) -> None:
        """
        Writes the bytes into the shared memory, blocks until writing is complete
        This function also releases the GIL, while writing to the shared memory
        """
        pass

    def write_async(self, data: bytes) -> None:
        """
        Sends the bytes to a background thread to write into the shared memory
        This method will never block the current thread
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


class SharedCircularQueue:
    @staticmethod
    def create(name: str, max_element_size: int, capacity: int,
               mode: OpenMode = OpenMode.ReadWrite) -> 'SharedCircularQueue':
        pass

    @staticmethod
    def open(name: str, mode: OpenMode = OpenMode.ReadWrite) -> 'SharedCircularQueue':
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


class SharedQueue:
    @staticmethod
    def create(name: str, max_element_size: int, mode: OpenMode = OpenMode.ReadWrite,
               buffer_size=8) -> 'SharedQueue':
        pass

    @staticmethod
    def open(name: str, mode: OpenMode = OpenMode.ReadWrite) -> 'SharedQueue':
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

    def is_empty(self) -> bool:
        """
        Because of the multiprocessing nature of the queue, this method is not reliable
        :returns: true if the queue is empty
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

def read_all(readers: list[SharedMessage]) -> list[bytes]:
    """
    Reads all the readers and returns a list of the messages
    Each message is read concurrently
    :param readers: list of readers
    :return: list of messages
    """
    return [reader.try_read() for reader in readers]

def read_all_map(readers: list[SharedMessage], map) -> list:
    """
    Reads all the readers and returns a list of the messages
    Each message is read concurrently
    :param readers: list of readers
    :param map: function to apply to the message
    :return: list of messages
    """
    return [map(reader.try_read()) for reader in readers]
