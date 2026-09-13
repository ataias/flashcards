# E2e smoke: Playwright + Lightpanda against the real binary

Unit/HTTP tests can stay green while the real `flashcards` process is unusable (e.g. intermediate Users stack states). CI therefore runs a minimal Playwright smoke via Lightpanda CDP against a freshly built binary on the runner. The smoke must track the product happy path in the same PR that changes it; Code Reviewer enforces that.
