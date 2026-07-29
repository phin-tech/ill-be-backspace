package main

func Sync(service string) {
	// Sync pulls files; it does NOT recreate containers. That is enough for
	// config a service reads live, but a compose change needs an up to take
	// effect. Verified 2026-07-29: fixing the data mount and force-syncing
	// left the same container id running the old mount, so the crash loop
	// continued. Hence the two-step below. See also the deploy runbook.
	pull(service)
	up(service)
}
