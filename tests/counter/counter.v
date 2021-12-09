module top ();
  wire [3:0] cout;
  wire clk;

  MCPNR_SWITCHES #(
    .POS_X(0),
    .POS_Y(0),
    .POS_Z(0),
    .NSWITCH(1),
  ) input_switches (
    .O({clk})
  );

  MCPNR_LIGHTS #(
    .POS_X(0),
    .POS_Y(8),
    .POS_Z(0),
    .NLIGHT(4)
  ) output_lights (
    .I({cout})
  );

  test_counter dut (.CLK(clk), .COUNT(cout));
endmodule

module test_counter (
  input            CLK,
  output reg [3:0] COUNT,
);
  always @(posedge CLK)
  begin
    COUNT <= COUNT + 1;
  end
endmodule
