def bridge():
    yield "start"
    for i in range(3):
        received = yield i
        if received:
            print("got", received)
    yield from ("a", "b")
    
g = bridge()
print(next(g))         # "start"
print(next(g))         # 0
print(g.send("hello")) # prints "got hello", returns 1
print(list(g))         # exhaust remaining values