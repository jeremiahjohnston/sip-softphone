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
    SP_EV_INCOMING = 2,   /* call_id, remote = caller URI */
    SP_EV_CALL_STATE = 3, /* call_id, code = SIP status, text = state name */
    SP_EV_MEDIA = 4,      /* call_id, code = 1 when audio is flowing */
    SP_EV_LOG = 5         /* text = log line */
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
/* Use no sound device (for headless tests). */
int  sp_use_null_audio(void);
void sp_shutdown(void);

#ifdef __cplusplus
}
#endif

#endif
