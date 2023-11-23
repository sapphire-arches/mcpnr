`default_nettype none
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
  wire [33:0] ibus_i_full = { ibus_rdt, ibus_ack };
  wire [33:0] ibus_o_full = { ibus_adr, ibus_cyc };

  // Data bus wires
  wire [31:0] o_dbus_adr;
  wire [31:0] o_dbus_dat;
  wire [3:0]  o_dbus_sel;
  wire        o_dbus_we;
  wire        o_dbus_cyc;
  wire [31:0] i_dbus_rdt;
  wire        i_dbus_ack;
  wire [33:0] dbus_i_full                    = { i_dbus_rdt, i_dbus_ack };
  wire [(32 + 32 + 4 + 1 + 1):0] dbus_o_full = { o_dbus_adr, o_dbus_dat, o_dbus_sel, o_dbus_we, o_dbus_cyc };

  // Collcetion of output wires
  wire [(2 + 6 * 2 + 4 + 6 * 2):0] regfile_outputs = {
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
  };

  // Layout of lights and switches, by tier:
  //     0       8      16      24      32      40      48      56      64      72      80      88      96     104     112     120     128     136     144
  // 0   |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |
  //     F D0D1  RF_O........................................................          IBUS_I............................................................
  // 1   |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |
  //     C R I M DBUS_I............................................................    IBUS_O............................................................
  // 2   |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |
  //     DBUS_O......................................................................................................................................
  // 3   |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |       |
  //     
  //
  // F = rf_ready
  // D0,D1 = rdata0, rdata1
  // C = clock
  // R = reset
  // I = timer interrupt
  // M = mdu_valid
  //

  // Generic I/O pins
  MCPNR_SWITCHES #(
    .POS_X(0), .POS_Y(16), .POS_Z(0),
    .NSWITCH(1),
  ) clk_switch (
    .O({clk})
  );

  MCPNR_SWITCHES #(
    .POS_X(2), .POS_Y(16), .POS_Z(0),
    .NSWITCH(1),
  ) rst_switch (
    .O({reset})
  );

  MCPNR_SWITCHES #(
    .POS_X(4), .POS_Y(16), .POS_Z(0),
    .NSWITCH(1),
  ) timer_irq_switch (
    .O({timer_irq})
  );

  // Register file interface
  // TODO: replace this with an efficient "block ram" (hehe geddit)
  MCPNR_SWITCHES #(
    .POS_X(0), .POS_Y(0), .POS_Z(0),
    .NSWITCH(3),
  ) rf_switch (
    .O({rf_ready, rdata0, rdata1})
  );

  genvar i;
  generate
    for (i = 0; i < $size(regfile_outputs); i = i + 1) begin
      MCPNR_LIGHTS #(
        .POS_X(8 + 2*i), .POS_Y(0), .POS_Z(0),
        .NLIGHT(1),
      ) rf_light (
        .I(regfile_outputs[i])
      );
    end
  endgenerate

  // Data bus
  generate
    for (i = 0; i < 33; i = i + 1) begin
      MCPNR_SWITCHES #(
        .POS_X(8 + 2*i), .POS_Y(16), .POS_Z(0),
        .NSWITCH(1)
      ) data_switches (
        .O(dbus_i_full[i])
      );
    end
  endgenerate

  generate
    for (i = 0; i < $bits(dbus_o_full); i = i + 1) begin
      MCPNR_LIGHTS #(
        .POS_X(2 * i), .POS_Y(32), .POS_Z(0),
        .NLIGHT(1)
      ) data_lights (
        .I(dbus_o_full[i])
      );
    end
  endgenerate

  // Instruction bus
  generate
    for (i = 0; i < 33; i = i + 1) begin
      MCPNR_SWITCHES #(
        .POS_X(78 + 2*i), .POS_Y(0), .POS_Z(0),
        .NSWITCH(1)
      ) ibus_switches (
        .O(ibus_i_full[i])
      );
    end
  endgenerate

  generate
    for (i = 0; i < 33; i = i + 1) begin
      MCPNR_LIGHTS #(
        .POS_X(78 + 2*i), .POS_Y(16), .POS_Z(0),
        .NLIGHT(1)
      ) ibus_lights (
        .I(ibus_o_full[i])
      );
    end
  endgenerate

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
    .o_dbus_adr(o_dbus_adr),
    .o_dbus_dat(o_dbus_dat),
    .o_dbus_sel(o_dbus_sel),
    .o_dbus_we (o_dbus_we ),
    .o_dbus_cyc(o_dbus_cyc),
    .i_dbus_rdt(i_dbus_rdt),
    .i_dbus_ack(i_dbus_ack),
  );
endmodule
