/*
 * Thin C layer over pjsua. Rust talks only to these functions, so it never
 * has to mirror pjsua's large config structs.
 */
#ifndef SP_SHIM_H
#define SP_SHIM_H

#ifdef __cplusplus
extern "C" {
#endif

enum sp_event_type {
    SP_EV_REG = 1,        /* code = SIP status, text = reason, call_id = 1 if registered */
    SP_EV_INCOMING = 2,   /* call_id, remote = caller URI, text = "intercom" if the PBX asked to auto-answer */
    SP_EV_CALL_STATE = 3, /* call_id, code = SIP status, text = state name */
    SP_EV_MEDIA = 4,      /* call_id, code = 1 when audio is flowing */
    SP_EV_LOG = 5,        /* text = log line */
    SP_EV_TRANSFER = 6,   /* call_id, code = SIP status of the transfer, text = reason, remote = "final" when done */
    SP_EV_MWI = 7,        /* text = message-summary body (RFC 3842) from the voicemail server */
    SP_EV_PRESENCE = 8,   /* remote = watched number, text = "idle", "ringing", "busy" or "unknown" */
    SP_EV_REJECTED = 9    /* remote = caller URI, code = SIP status sent, text = "dnd" or "forwarded" */
};

typedef struct sp_event {
    int type;
    int call_id;
    int code;
    const char *text;
    const char *remote;
} sp_event;

/* Called from pjsua worker threads. Strings are only valid during the call. */
typedef void (*sp_event_cb)(const sp_event *ev, void *user);

int  sp_init(sp_event_cb cb, void *user, int log_level);
/* transport: "udp", "tcp" or "tls". port 0 means the default for that transport. */
int  sp_set_account(const char *server, int port, const char *transport,
                    const char *user, const char *auth_user,
                    const char *password, const char *display_name);
int  sp_remove_account(void);
/* dest may be a bare extension/number or a full sip: URI. Returns call id or -1. */
int  sp_make_call(const char *dest);
int  sp_answer(int call_id);
int  sp_hangup(int call_id);
void sp_hangup_all(void);
int  sp_send_dtmf(int call_id, const char *digits);
int  sp_set_mute(int call_id, int mute);
int  sp_set_hold(int call_id, int hold);
/* Do Not Disturb: reject new incoming calls with 486 Busy Here. */
void sp_set_dnd(int on);
/* Forward every incoming call (302 redirect) to dest; NULL or "" turns it off. */
void sp_set_forward(const char *dest);
/* BLF: watch an extension's call state (SUBSCRIBE to the dialog event
 * package). Results arrive as SP_EV_PRESENCE. Returns 0 on success. */
int  sp_watch(const char *number);
/* Stops watching everything. */
void sp_unwatch_all(void);
/* Blind transfer to dest (same forms as sp_make_call). */
int  sp_transfer(int call_id, const char *dest);
/* Attended transfer: connect call_id's party to the party on dest_call_id. */
int  sp_transfer_attended(int call_id, int dest_call_id);
/* Audio devices. Re-scans hardware first when no call is active.
 * Returns the number of devices. */
int  sp_audio_dev_count(void);
/* Fills name/driver for device idx. Returns 0 on success. */
int  sp_audio_dev_info(int idx, char *name, int name_len, char *driver,
                       int driver_len, int *inputs, int *outputs);
/* -1 for either means the system default. */
int  sp_set_audio_devs(int capture, int playback);
/* Ringer through the selected speaker. mode: 0 off, 1 ring (loops),
 * 2 call-waiting beep (plays once), 3 preview (rings once).
 * tone picks one of sp_ringtone_count() built-in ringtones. */
int  sp_ring(int mode, int tone);
int  sp_ringtone_count(void);
/* Use no sound device (for headless tests). */
int  sp_use_null_audio(void);
void sp_shutdown(void);

#ifdef __cplusplus
}
#endif

#endif
