# PJSIP patches

Fixes applied on top of the PJSIP release by the `build-pjsip-*` scripts.
Each one is applied once; rerunning a script on an existing checkout skips
patches that are already in.

- `mwi-sync-send-failure.patch`: when the voicemail (MWI) SUBSCRIBE can't be
  sent at all, for example because the PBX host is down and the OS reports
  "host is down" straight away, the subscription ends inside
  `pjsua_start_mwi()` and clears the dialog pointer that function then
  unlocks. That trips an assertion and aborts the app, every time the
  five-minute MWI retry timer lands while the PBX is unreachable. The patch
  keeps a local pointer to the dialog it locked, like PJSIP already does for
  buddy subscriptions. Not fixed upstream as of 2.17.
