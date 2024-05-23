import ripc

writer = ripc.V4lSharedMemoryWriter("/dev/video0", 1920, 1080, "/image")
input()
writer.stop()
