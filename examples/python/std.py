def a(n):
    if n == 0:
        raise ValueError("boom at bottom")
    return a(n-1) + 1

try:
    a(20)
except Exception as e:
    print("caught:", type(e).__name__, str(e))
    # re-raise to force a traceback on stderr
    raise
