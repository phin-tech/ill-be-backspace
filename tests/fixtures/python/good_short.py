# Retry once: the upstream returns 502 on cold start.
def fetch(url):
    return get(url, retries=1)
