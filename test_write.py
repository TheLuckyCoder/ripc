import time
import ripc
import cv2

images = [cv2.imread('img_0.png'), cv2.imread('img_1.png'), cv2.imread('img_2.png')]

writer = ripc.SharedMemoryWriter("image", 100 * 1024 * 1024)
index = 0
print(f"{writer.total_allocated_size() = }; {writer.size() = }")
input()
while True:
    tobytes = images[index % 3].tobytes()
    start_time = time.perf_counter()
    writer.write(tobytes)
    end_time = time.perf_counter() - start_time
    index += 1
    print("Written " + str(index % 3) + " Time: " + str(end_time * 1000))
    time.sleep(0.01)
