#!/usr/bin/env bash
set -eu

function usage() {
  set +x
  echo "Usage"
  echo "  $1 <path to serv> <output prefix>"
}

if [[ -z ${1+x} || -z ${2+x} ]]
then
  usage $0
  exit 1
fi

SERV_DIR=$1
MY_DIR=$(dirname $0)

SERV_FILES="
serv_aligner.v
serv_alu.v
serv_bufreg.v
serv_bufreg2.v
serv_compdec.v
serv_csr.v
serv_ctrl.v
serv_decode.v
serv_immdec.v
serv_mem_if.v
serv_rf_if.v
serv_rf_ram.v
serv_rf_ram_if.v
serv_rf_top.v
serv_state.v
serv_synth_wrapper.v
serv_top.v
"

echo [ ] Running from ${MY_DIR}

set -x

BUILD_DIR=$(mktemp -d)
trap "rm -rf ${BUILD_DIR}" 0 2 3 15

YOSYS_SYNTH_MC=${MY_DIR}/../../yosys-synth_mc/
MC_TECHLIB=${YOSYS_SYNTH_MC}techlib
YOSYS_SCRIPT_FILE=${BUILD_DIR}/script.ys
YOSYS_LOG_FILE=${BUILD_DIR}/log.txt
MCPNR_RUST_TOOLS=${MY_DIR}/../../pnr/target/release/

echo 'plugin -i' ${YOSYS_SYNTH_MC}/synth_mc.so >> ${YOSYS_SCRIPT_FILE}

for f in ${SERV_FILES}
do
  echo read_verilog ${SERV_DIR}/$f >> ${YOSYS_SCRIPT_FILE}
done
echo read_verilog ${MY_DIR}/mc_io.v >> ${YOSYS_SCRIPT_FILE}

echo synth_mc -flatten -top mc_io_top -techlib ${MC_TECHLIB} >> ${YOSYS_SCRIPT_FILE}
echo stat -liberty ${MC_TECHLIB}/minecraft.lib >> ${YOSYS_SCRIPT_FILE}
echo rename mc_io_top top >> ${YOSYS_SCRIPT_FILE}
echo write_json ${BUILD_DIR}/${2}.yosys-design >> ${YOSYS_SCRIPT_FILE}

yosys -s ${YOSYS_SCRIPT_FILE} | tee ${YOSYS_LOG_FILE}

cp ${BUILD_DIR}/${2}.yosys-design .

SIZE_X=144
SIZE_Z=128
TIERS=4

${MCPNR_RUST_TOOLS}/mcpnr-placement place --size-y ${TIERS} --size-x ${SIZE_X} --size-z ${SIZE_Z} --techlib ${MC_TECHLIB} ${BUILD_DIR}/${2}.yosys-design ${BUILD_DIR}/${2}.mcpnr-placement
${MCPNR_RUST_TOOLS}/mcpnr-routing --tiers ${TIERS} --techlib ${MC_TECHLIB} ${BUILD_DIR}/${2}.mcpnr-placement ${BUILD_DIR}/${2}.mcpnr-routed
python3 ${MY_DIR}/../../pnr/routed-to-world/convert.py ${BUILD_DIR}/${2}.mcpnr-routed "${HOME}/.local/share/PrismLauncher/instances/1.20.2/.minecraft/saves/Designs"
