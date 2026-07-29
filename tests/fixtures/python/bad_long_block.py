def sync(service):
    # Sync pulls files; it does NOT recreate containers. That's enough for
    # config a service reads live (Traefik file-watches dynamic/, Dashy
    # re-reads conf.yml per request), but a compose change needs an `up` to
    # take effect. Verified 2026-07-29: fixing the data mount and force-syncing
    # left the same container id running the old mount, so the crash loop
    # continued. Hence the two-step below.
    pull(service)
    up(service)
