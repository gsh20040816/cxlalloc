package CAS_types;

typedef enum logic [2:0] {
    IDLE,
    RD_REQ,
    RD_RESP,
    COMPARE,
    WR_REQ,
    WR_RESP
} cas_state_t;

// typedef enum logic [2:0] {
//     STATE_WR_RESET,
//     STATE_WR_SUB,
//     STATE_WR_SUB_RESP,
//     STATE_WR_ORIG,
//     STATE_WR_ORIG_RESP
// } wr_state_t;

// typedef enum logic [1:0] {
//     STATE_RD_RESET,
//     STATE_RD_DATA,
//     STATE_RD_DONE
// } rd_state_t;

endpackage