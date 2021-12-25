module top ();
  wire [7:0] a, b, y;

  MCPNR_SWITCHES #(
    .POS_X(0),
    .POS_Y(0),
    .POS_Z(0),
    .NSWITCH(16),
  ) input_switches (
    .O({a, b}),
  );

  MCPNR_LIGHTS #(
    .POS_X(0),
    .POS_Y(7),
    .POS_Z(0),
    .NLIGHT(8),
  ) output_lights (
    .I({y})
  );

  test_nor dut (.A(a), .B(b), .Y(y));
endmodule

module test_nor (
  input  [7:0] A, B,
  output [7:0] Y,
);
  assign Y = ~(B | A);
endmodule
