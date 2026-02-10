package CAS_types;

typedef struct packed {
    // 6 x 4  = 24
    logic [5:0] aruser;
    logic [5:0] awuser;
    logic [5:0] awsize;
    logic [5:0] awaddr;
} axi_config_bits_t;

typedef enum logic [2:0] {
    STATE_WR_RESET,
    STATE_WR_SUB,
    STATE_WR_SUB_RESP,
    STATE_WR_ORIG,
    STATE_WR_ORIG_RESP
} wr_state_t;

typedef enum logic [1:0] {
    STATE_RD_RESET,
    STATE_RD_DATA,
    STATE_RD_DONE
} rd_state_t;

endpackage