def make_counter(start=0):
    def inc(step=1):
        nonlocal start
        start += step
        return start
    return inc

c = make_counter(10)
print(c())      # 11
print(c(5))     # 16