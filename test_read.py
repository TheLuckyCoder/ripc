import time

import ripc
import cv2
import numpy

reader = ripc.SharedMemoryReader("/image")
print("initialized")
while True:
    start_time = time.perf_counter()
    byte = reader.read_in_place(False)
    end_time = time.perf_counter() - start_time
    print(end_time * 1000)
    print(len(byte))
    image = numpy.frombuffer(byte, dtype=numpy.uint8).reshape(2160, 3840, 3)
    # cv2.imwrite("output.png", image)
