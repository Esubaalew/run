class Resource:
    def __init__(self, name):
        self.name = name
    def __repr__(self):
        return f"<Resource {self.name}>"
    def __enter__(self):
        print("enter", self.name)
        return self
    def __exit__(self, exc_type, exc, tb):
        print("exit", self.name)
        return False  # do not suppress exceptions

with Resource("X") as r:
    print("inside", r)