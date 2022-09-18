module top (
);
  wire [7:0] a, b;
  wire [8:0] y;

  MCPNR_SWITCHES #(
    .POS_X(0),
    .POS_Y(0),
    .POS_Z(0),
    .NSWITCH(16),
  ) input_switches (
    .O({a, b}),
  );

  MCPNR_LIGHTS #(
    .POS_X(64),
    .POS_Y(0),
    .POS_Z(0),
    .NLIGHT(9),
  ) output_lights (
    .I({y})
  );

  test_adder dut(.A(a), .B(b), .Y(y));
endmodule

module test_adder (
  input  [7:0] A, B,
  output [8:0] Y
);
  assign Y = A + B;
endmodule
