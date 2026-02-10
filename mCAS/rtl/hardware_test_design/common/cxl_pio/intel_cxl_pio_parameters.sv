// (C) 2001-2024 Intel Corporation. All rights reserved.
// Your use of Intel Corporation's design tools, logic functions and other 
// software and tools, and its AMPP partner logic functions, and any output 
// files from any of the foregoing (including device programming or simulation 
// files), and any associated documentation or information are expressly subject 
// to the terms and conditions of the Intel Program License Subscription 
// Agreement, Intel FPGA IP License Agreement, or other applicable 
// license agreement, including, without limitation, that your use is for the 
// sole purpose of programming logic devices manufactured by Intel and sold by 
// Intel or its authorized distributors.  Please refer to the applicable 
// agreement for further details.


package intel_cxl_pio_parameters;
    parameter ENABLE_ONLY_DEFAULT_CONFIG= 0;
    parameter ENABLE_ONLY_PIO           = 0;
    parameter ENABLE_BOTH_DEFAULT_CONFIG_PIO = 1;
    parameter PFNUM_WIDTH               = 3;
    parameter VFNUM_WIDTH               = 12;
    parameter DATA_WIDTH                = 1024;
    parameter BAM_DATAWIDTH             = DATA_WIDTH;
    parameter DEVICE_FAMILY             = "Agilex";
    //parameter CXL_IO_DWIDTH = 256; // Data width for each channel
    //parameter CXL_IO_PWIDTH = 32;  // Prefix Width
    //parameter CXL_IO_CHWIDTH = 1;  // Prefix Width

endpackage
`ifdef QUESTA_INTEL_OEM
`pragma questa_oem_00 "EtAh8aN7m2BPKOTfO5tEAbNSD19BnNEklF4xQRY7YZ2oRe/8wDIRx8XCKuwkXQtjYcM5gRXSD6c+oGX77mfnvlAGw9KTmnXPBu3GU7e3qFjUTrXWlEAN76gMqJTePk91Iv2qtpAKuY2LJHLiowUVDoSuAt1Csh1O2u7qDzQRIaeVL/AJWYDMfWERE2K26wZcHHB8eTbMnhSND4m01aQODfKXixyUFYBUVJCy/gZrUwDhE5mPyZ7WxEi7qQa5efvNme5O3uze8FsfwLp5iIeud/OzmPk8MiNBzkmg4F7jzs741AcwHJ2mgk2WLCOraEGffWLbbZCMsKArusG9u/WvBVYlajVt5qcTWsKB6DEpD0qghh6xzZTqvWVITSWoIqXqQ+dXhrcU90sxe/5w+3TBKSpkaVrH5P2zbSgRXWfIQ5ZzeG5bYqxJMQNMBsqvhsdoaV4WFw56URUINbIm2yiR8eP5ExoCzTWqjZFuvbpXLXw0PqCWZrWBQg4zMOaaO9Q9vN5Pv/QPYUydQrfjm7oiKgpeUVJ05krEJflUagDt/lOGluhojxfQuWs2OI96TJBa4aRW1W5Q10VHtJutq8y+CGD5uapTEzf0ttFdeT86zt7dWqc6fjaeB42eD9alrLaWNAWZgjlsGAd9PGcQzQq4tU+5HqFwTiTVa9j6X0BCxTz+3sX1z0aNeW1doHBSJ/HCAFIvtzrSobASj6kt9enwSMkOpws31aehBU3H3PRMbuKrJJG7QX1g5Tp4OTDgGAokAwHds8drPaRqkFOUuj3t3X24/ESQ9VNlsyeoeTtQRMg0JYMpyIcUgFUyctkVpBauPa8f93ORVZzgCCE+2Ed5W0bZMaWmKan6cZ80TDLNaiu9UJPgx6mtq2LqTfRmQX9eeFthkEOTgdRSaK7eLYX/ihuEAggNvO3BkUaJnv8JA3Jy4+kYgJ9SCLwwUta56ycwAlcbefx4Ca7XD/m7SNEFGcAGNCYjIRcvQ2iMbYH5sBAlYWk6wPVlWyyzaJpswzvS"
`endif