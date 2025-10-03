src = """
def dynamic(x):
    return x*x
"""
exec(compile(src, "<dynamic>", "exec"), globals())
print(dynamic(7))   # 49