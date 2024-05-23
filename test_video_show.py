import time

import ripc
import cv2
import numpy

reader = ripc.SharedMemoryReader("/image")
print("initialized")
while True:
    start = time.perf_counter()
    byte = reader.read_in_place(False)
    # image = cv2.imdecode(numpy.frombuffer(byte, dtype=numpy.uint8), cv2.IMREAD_UNCHANGED)
    image = numpy.frombuffer(byte, dtype=numpy.uint8).reshape(1080, 1920, 3)
    print("Time: " + str((time.perf_counter() - start) * 1000))
    cv2.imshow("Video", image)
    cv2.imwrite("output.jpg", image)
