module test_counter (
  input            CLK,
  output reg [3:0] COUNT,
);
  always @(posedge CLK)
  begin
    COUNT <= COUNT + 1;
  end
endmodule
