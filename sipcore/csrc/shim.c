#include "shim.h"

#include <pjsua-lib/pjsua.h>
#include <stdio.h>
#include <string.h>

#if defined(_MSC_VER)
#  define SP_TLS __declspec(thread)
#else
#  define SP_TLS __thread
#endif

static sp_event_cb g_cb;
static void *g_user;
static pjsua_acc_id g_acc = PJSUA_INVALID_ID;
static pjsua_transport_id g_tp_udp = PJSUA_INVALID_ID;
static pjsua_transport_id g_tp_tcp = PJSUA_INVALID_ID;
static pjsua_transport_id g_tp_tls = PJSUA_INVALID_ID;
static char g_server[256];
static int g_port;
static char g_transport[8];
static pj_bool_t g_muted[PJSUA_MAX_CALLS];
static pj_bool_t g_started;

static void emit(int type, int call_id, int code, const char *text, const char *remote)
{
    sp_event ev;
    if (!g_cb)
        return;
    ev.type = type;
    ev.call_id = call_id;
    ev.code = code;
    ev.text = text ? text : "";
    ev.remote = remote ? remote : "";
    g_cb(&ev, g_user);
}

/* Threads created outside pjsip (the Rust/Tauri command threads) must be
 * registered with pjlib before they call into pjsua. */
static void ensure_thread(void)
{
    static SP_TLS pj_thread_desc desc;
    static SP_TLS pj_thread_t *thread;
    if (!pj_thread_is_registered())
        pj_thread_register("sp-ext", desc, &thread);
}

static pj_bool_t valid_call(int call_id)
{
    return call_id >= 0 && call_id < (int)PJSUA_MAX_CALLS &&
           pjsua_call_is_active(call_id);
}

static void on_log(int level, const char *data, int len)
{
    char buf[1024];
    PJ_UNUSED_ARG(level);
    if (len >= (int)sizeof(buf))
        len = sizeof(buf) - 1;
    memcpy(buf, data, len);
    /* Drop the trailing newline pjlib adds. */
    while (len > 0 && (buf[len - 1] == '\n' || buf[len - 1] == '\r'))
        len--;
    buf[len] = '\0';
    emit(SP_EV_LOG, -1, level, buf, NULL);
}

static void on_reg_state2(pjsua_acc_id acc_id, pjsua_reg_info *info)
{
    pjsua_acc_info ai;
    char reason[128];
    if (pjsua_acc_get_info(acc_id, &ai) != PJ_SUCCESS)
        return;
    snprintf(reason, sizeof(reason), "%.*s",
             (int)ai.status_text.slen, ai.status_text.ptr);
    emit(SP_EV_REG, info->renew && ai.status / 100 == 2 ? 1 : 0,
         ai.status, reason, NULL);
}

static const char *state_name(pjsip_inv_state s)
{
    switch (s) {
    case PJSIP_INV_STATE_CALLING:      return "calling";
    case PJSIP_INV_STATE_INCOMING:     return "incoming";
    case PJSIP_INV_STATE_EARLY:        return "ringing";
    case PJSIP_INV_STATE_CONNECTING:   return "connecting";
    case PJSIP_INV_STATE_CONFIRMED:    return "confirmed";
    case PJSIP_INV_STATE_DISCONNECTED: return "disconnected";
    default:                           return "idle";
    }
}

static void on_incoming_call(pjsua_acc_id acc_id, pjsua_call_id call_id,
                             pjsip_rx_data *rdata)
{
    pjsua_call_info ci;
    char remote[256];
    PJ_UNUSED_ARG(acc_id);
    PJ_UNUSED_ARG(rdata);
    g_muted[call_id] = PJ_FALSE;
    pjsua_call_get_info(call_id, &ci);
    snprintf(remote, sizeof(remote), "%.*s",
             (int)ci.remote_info.slen, ci.remote_info.ptr);
    pjsua_call_answer(call_id, 180, NULL, NULL);
    emit(SP_EV_INCOMING, call_id, 180, "incoming", remote);
}

static void on_call_state(pjsua_call_id call_id, pjsip_event *e)
{
    pjsua_call_info ci;
    char remote[256];
    PJ_UNUSED_ARG(e);
    pjsua_call_get_info(call_id, &ci);
    snprintf(remote, sizeof(remote), "%.*s",
             (int)ci.remote_info.slen, ci.remote_info.ptr);
    emit(SP_EV_CALL_STATE, call_id, ci.last_status, state_name(ci.state), remote);
}

static void connect_audio(pjsua_call_id call_id, pjsua_conf_port_id slot)
{
    pjsua_conf_connect(slot, 0);
    if (!g_muted[call_id])
        pjsua_conf_connect(0, slot);
}

static void on_call_media_state(pjsua_call_id call_id)
{
    pjsua_call_info ci;
    pj_bool_t active;
    pjsua_call_get_info(call_id, &ci);
    active = ci.media_status == PJSUA_CALL_MEDIA_ACTIVE;
    if (active)
        connect_audio(call_id, ci.conf_slot);
    emit(SP_EV_MEDIA, call_id, active ? 1 : 0,
         ci.media_status == PJSUA_CALL_MEDIA_LOCAL_HOLD ? "local_hold" :
         ci.media_status == PJSUA_CALL_MEDIA_REMOTE_HOLD ? "remote_hold" :
         active ? "active" : "none",
         NULL);
}

int sp_init(sp_event_cb cb, void *user, int log_level)
{
    pjsua_config cfg;
    pjsua_logging_config log_cfg;
    pjsua_media_config media_cfg;
    pjsua_transport_config tcfg;
    pj_status_t status;

    if (g_started)
        return 0;
    g_cb = cb;
    g_user = user;

    status = pjsua_create();
    if (status != PJ_SUCCESS)
        return status;

    pjsua_config_default(&cfg);
    cfg.cb.on_reg_state2 = &on_reg_state2;
    cfg.cb.on_incoming_call = &on_incoming_call;
    cfg.cb.on_call_state = &on_call_state;
    cfg.cb.on_call_media_state = &on_call_media_state;
    cfg.user_agent = pj_str("SIP Softphone");

    pjsua_logging_config_default(&log_cfg);
    log_cfg.level = log_level;
    log_cfg.console_level = log_level; /* pjsua only forwards up to this level to cb */
    log_cfg.cb = &on_log;

    pjsua_media_config_default(&media_cfg);
    media_cfg.ec_tail_len = 200;

    status = pjsua_init(&cfg, &log_cfg, &media_cfg);
    if (status != PJ_SUCCESS)
        goto fail;

    pjsua_transport_config_default(&tcfg);
    tcfg.port = 0; /* any free local port */
    pjsua_transport_create(PJSIP_TRANSPORT_UDP, &tcfg, &g_tp_udp);
    pjsua_transport_create(PJSIP_TRANSPORT_TCP, &tcfg, &g_tp_tcp);
#if defined(PJSIP_HAS_TLS_TRANSPORT) && PJSIP_HAS_TLS_TRANSPORT
    pjsua_transport_create(PJSIP_TRANSPORT_TLS, &tcfg, &g_tp_tls);
#endif
    if (g_tp_udp == PJSUA_INVALID_ID && g_tp_tcp == PJSUA_INVALID_ID) {
        status = PJ_EINVALIDOP;
        goto fail;
    }

    status = pjsua_start();
    if (status != PJ_SUCCESS)
        goto fail;

    /* No audio hardware at all: keep signalling working instead of failing
     * every call. */
    if (pjmedia_aud_dev_count() == 0)
        pjsua_set_null_snd_dev();

    g_started = PJ_TRUE;
    return 0;

fail:
    pjsua_destroy();
    return status;
}

int sp_set_account(const char *server, int port, const char *transport,
                   const char *user, const char *auth_user,
                   const char *password, const char *display_name)
{
    pjsua_acc_config acc;
    char id[512], reg_uri[512], tp_param[32];
    pjsua_transport_id tp;
    pj_status_t status;

    if (!g_started)
        return PJ_EINVALIDOP;
    ensure_thread();
    sp_remove_account();

    if (transport && strcmp(transport, "tcp") == 0) {
        tp = g_tp_tcp;
        snprintf(tp_param, sizeof(tp_param), ";transport=tcp");
    } else if (transport && strcmp(transport, "tls") == 0) {
        tp = g_tp_tls;
        snprintf(tp_param, sizeof(tp_param), ";transport=tls");
    } else {
        tp = g_tp_udp;
        tp_param[0] = '\0';
    }
    if (tp == PJSUA_INVALID_ID)
        return PJ_ENOTSUP;

    snprintf(g_server, sizeof(g_server), "%s", server);
    snprintf(g_transport, sizeof(g_transport), "%s", tp_param[0] ? transport : "udp");
    g_port = port;

    if (display_name && *display_name)
        snprintf(id, sizeof(id), "\"%s\" <sip:%s@%s>", display_name, user, server);
    else
        snprintf(id, sizeof(id), "<sip:%s@%s>", user, server);
    if (port > 0)
        snprintf(reg_uri, sizeof(reg_uri), "sip:%s:%d%s", server, port, tp_param);
    else
        snprintf(reg_uri, sizeof(reg_uri), "sip:%s%s", server, tp_param);

    pjsua_acc_config_default(&acc);
    acc.id = pj_str(id);
    acc.reg_uri = pj_str(reg_uri);
    acc.transport_id = tp;
    acc.cred_count = 1;
    acc.cred_info[0].realm = pj_str("*");
    acc.cred_info[0].scheme = pj_str("digest");
    acc.cred_info[0].username = pj_str((char *)(auth_user && *auth_user ? auth_user : user));
    acc.cred_info[0].data_type = PJSIP_CRED_DATA_PLAIN_PASSWD;
    acc.cred_info[0].data = pj_str((char *)password);

    /* pjsua copies the config, so the stack buffers above are fine. */
    status = pjsua_acc_add(&acc, PJ_TRUE, &g_acc);
    return status;
}

int sp_remove_account(void)
{
    pj_status_t status = PJ_SUCCESS;
    if (!g_started)
        return PJ_EINVALIDOP;
    ensure_thread();
    if (g_acc != PJSUA_INVALID_ID) {
        status = pjsua_acc_del(g_acc);
        g_acc = PJSUA_INVALID_ID;
    }
    return status;
}

int sp_make_call(const char *dest)
{
    char uri[512];
    pj_str_t target;
    pjsua_call_id call_id = PJSUA_INVALID_ID;
    pj_status_t status;

    if (!g_started || g_acc == PJSUA_INVALID_ID || !dest || !*dest)
        return -1;
    ensure_thread();

    if (strncmp(dest, "sip:", 4) == 0 || strncmp(dest, "sips:", 5) == 0) {
        snprintf(uri, sizeof(uri), "%s", dest);
    } else {
        char port[16] = "";
        if (g_port > 0)
            snprintf(port, sizeof(port), ":%d", g_port);
        if (strcmp(g_transport, "udp") == 0)
            snprintf(uri, sizeof(uri), "sip:%s@%s%s", dest, g_server, port);
        else
            snprintf(uri, sizeof(uri), "sip:%s@%s%s;transport=%s",
                     dest, g_server, port, g_transport);
    }

    target = pj_str(uri);
    status = pjsua_call_make_call(g_acc, &target, NULL, NULL, NULL, &call_id);
    if (status != PJ_SUCCESS)
        return -1;
    g_muted[call_id] = PJ_FALSE;
    return call_id;
}

int sp_answer(int call_id)
{
    if (!valid_call(call_id))
        return PJ_EINVAL;
    ensure_thread();
    return pjsua_call_answer(call_id, 200, NULL, NULL);
}

int sp_hangup(int call_id)
{
    if (!valid_call(call_id))
        return PJ_EINVAL;
    ensure_thread();
    return pjsua_call_hangup(call_id, 0, NULL, NULL);
}

void sp_hangup_all(void)
{
    if (!g_started)
        return;
    ensure_thread();
    pjsua_call_hangup_all();
}

int sp_send_dtmf(int call_id, const char *digits)
{
    pjsua_call_send_dtmf_param p;
    if (!valid_call(call_id) || !digits)
        return PJ_EINVAL;
    ensure_thread();
    pjsua_call_send_dtmf_param_default(&p);
    p.method = PJSUA_DTMF_METHOD_RFC2833;
    p.digits = pj_str((char *)digits);
    return pjsua_call_send_dtmf(call_id, &p);
}

int sp_set_mute(int call_id, int mute)
{
    pjsua_conf_port_id slot;
    if (!valid_call(call_id))
        return PJ_EINVAL;
    ensure_thread();
    g_muted[call_id] = mute ? PJ_TRUE : PJ_FALSE;
    slot = pjsua_call_get_conf_port(call_id);
    if (slot == PJSUA_INVALID_ID)
        return 0; /* applied when media comes up */
    if (mute)
        return pjsua_conf_disconnect(0, slot);
    return pjsua_conf_connect(0, slot);
}

int sp_set_hold(int call_id, int hold)
{
    if (!valid_call(call_id))
        return PJ_EINVAL;
    ensure_thread();
    if (hold)
        return pjsua_call_set_hold(call_id, NULL);
    return pjsua_call_reinvite(call_id, PJSUA_CALL_UNHOLD, NULL);
}

int sp_use_null_audio(void)
{
    if (!g_started)
        return PJ_EINVALIDOP;
    ensure_thread();
    return pjsua_set_null_snd_dev();
}

void sp_shutdown(void)
{
    if (!g_started)
        return;
    ensure_thread();
    g_started = PJ_FALSE;
    g_acc = PJSUA_INVALID_ID;
    pjsua_destroy();
    g_cb = NULL;
}
