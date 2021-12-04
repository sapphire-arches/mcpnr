// The names of these modules should correspond exactly with structure files in
// ./structures

module gate_nand_i2 (
  input I0, I1,
  output O0
);
  assign O0 = !(I0 & I1);
endmodule

module gate_nor_i2 (
  input I0, I1,
  output O0
);
  assign O0 = !(I0 | I1)
endmodule

module gate_not (
  input I0,
  output O0
);
  assign O0 = !I0
endmodule
