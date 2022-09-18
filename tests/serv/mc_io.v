module mc_io_top (
);
  wire clk, reset, timer_irq;

  // Register file wires
  wire rf_ready, rdata0, rdata1;
  wire rf_rreq;
  wire rf_wreq;
  wire [5:0] wreg0;
  wire [5:0] wreg1;
  wire wen0;
  wire wen1;
  wire wdata0;
  wire wdata1;
  wire [5:0] rreg0;
  wire [5:0] rreg1;

  // Instruction bus wires
  wire [31:0] ibus_adr;
  wire        ibus_cyc;
  wire [31:0] ibus_rdt;
  wire        ibus_ack;

  // Data bus wires
  wire [31:0] o_dbus_adr;
  wire [31:0] o_dbus_dat;
  wire [3:0]  o_dbus_sel;
  wire        o_dbus_we;
  wire        o_dbus_cyc;
  wire [31:0] i_dbus_rdt;
  wire        i_dbus_ack;

  // Extension bus wires
  wire [ 2:0] ext_funct3;
  wire        ext_ready;
  wire [31:0] ext_rd;
  wire [31:0] ext_rs1;
  wire [31:0] ext_rs2;
  //MDU
  wire        o_mdu_valid;

  // Generic I/O pins
  MCPNR_SWITCHES #(
    .POS_X(0), .POS_Y(0), .POS_Z(0),
    .NSWITCH(3),
  ) clk_switch (
    .O({clk, reset, timer_irq})
  );

  // Register file interface
  // TODO: replace this with an efficient "block ram" (hehe geddit)
  MCPNR_SWITCHES #(
    .POS_X(16), .POS_Y(0), .POS_Z(0),
    .NSWITCH(3),
  ) rf_switch (
    .O({rf_ready, rdata0, rdata1})
  );

  MCPNR_LIGHTS #(
    .POS_X(16), .POS_Y(8), .POS_Z(0),
    .NLIGHT(2 + 6 * 2 + 4 + 6 * 2),
  ) rf_light (
    .I({
      rf_rreq,
      rf_wreq,
      wreg0,
      wreg1,
      wen0,
      wen1,
      wdata0,
      wdata1,
      rreg0,
      rreg1
    })
  );

  // Instruction bus
  MCPNR_SWITCHES #(
    .POS_X(64), .POS_Y(0), .POS_Z(0),
    .NSWITCH(32 + 1)
  ) ibus_switches (
    .O({ibus_rdt, ibus_ack})
  );

  MCPNR_LIGHTS #(
    .POS_X(64), .POS_Y(8), .POS_Z(0),
    .NLIGHT(32 + 1)
  ) ibus_lights (
    .I({ibus_adr, ibus_cyc})
  );

  // Data bus
  MCPNR_SWITCHES #(
    .POS_X(112), .POS_Y(0), .POS_Z(0),
    .NSWITCH(32 + 1)
  ) data_switches (
    .O({dbus_rdt, dbus_ack})
  );

  MCPNR_LIGHTS #(
    .POS_X(112), .POS_Y(8), .POS_Z(0),
    .NLIGHT(32 + 32 + 4 + 1 + 1)
  ) data_lights (
    .I({dbus_adr, dbus_dat, dbus_sel, dbus_we, dbus_cyc})
  );

  // Extension bus
  MCPNR_SWITCHES #(
    .POS_X(192), .POS_Y(0), .POS_Z(0),
    .NSWITCH(1 + 32)
  ) ext_switches (
    .O({ext_ready, ext_rd})
  );

  MCPNR_LIGHTS #(
    .POS_X(192), .POS_Y(8), .POS_Z(0),
    .NLIGHT(3 + 32 + 32)
  ) ext_lights (
    .I({ext_funct3, ext_rs1, ext_rs2})
  );

  // MDU
  MCPNR_LIGHTS #(
    .POS_X(0), .POS_Y(32), .POS_Z(0),
    .NLIGHT(1)
  ) mdu_lights (
    .I(mdu_valid)
  );

  // SERV instantiation
  serv_top #() serv (
    .clk(clk),
    .i_rst(reset),
    .i_timer_irq(timer_irq),
    // RF interface
    .o_rf_rreq(rf_rreq),
    .o_rf_wreq(rf_wreq),
    .i_rf_ready(rf_ready),
    .o_wreg0(wreg0),
    .o_wreg1(wreg1),
    .o_wen0(wen0),
    .o_wen1(wen1),
    .o_wdata0(wdata0),
    .o_wdata1(wdata1),
    .o_rreg0(rreg0),
    .o_rreg1(rreg1),
    .i_rdata0(rdata0),
    .i_rdata1(rdata1),

    // Instruction bus
    .o_ibus_adr(ibus_adr),
    .o_ibus_cyc(ibus_cyc),
    .i_ibus_rdt(ibus_rdt),
    .i_ibus_ack(ibus_ack),
    // Data bus
    .o_dbus_adr(dbus_adr),
    .o_dbus_dat(dbus_dat),
    .o_dbus_sel(dbus_sel),
    .o_dbus_we (dbus_we ),
    .o_dbus_cyc(dbus_cyc),
    .i_dbus_rdt(dbus_rdt),
    .i_dbus_ack(dbus_ack),
    //Extension
    .o_ext_funct3(ext_funct3),
    .i_ext_ready(ext_ready),
    .i_ext_rd(ext_rd),
    .o_ext_rs1(ext_rs1),
    .o_ext_rs2(ext_rs2),
    //MDU
    .o_mdu_valid(mdu_valid)
  );
endmodule
