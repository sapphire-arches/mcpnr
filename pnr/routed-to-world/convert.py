import amulet
import amulet_nbt
import argparse
import json

from amulet.api.errors import ChunkDoesNotExist, ChunkLoadError

def parse_args():
    parser = argparse.ArgumentParser(description='Splat MCPNR router output JSON into a minecraft world')
    parser.add_argument('INFILE', help='Input JSON file from the mcpnr router')
    parser.add_argument('OUTPUT_WORLD', help='Path to the output world')

    parser.add_argument('--base-x', default=0, help='Base X coordinate for design splat')
    parser.add_argument('--base-y', default=3, help='Base Y coordinate for design splat')
    parser.add_argument('--base-z', default=0, help='Base Z coordinate for design splat')

    return parser.parse_args()

class WorldWrapper:
    """ Wrap Amulet world because the API is questionable """

    def __init__(self, world):
        self.world = world
        self.chunk_cache = {}
        self.BEDROCK = amulet.Block('minecraft', 'bedrock')

    def get_chunk(self, cx, cz):
        coords = (cx, cz)
        if coords in self.chunk_cache:
            return self.chunk_cache[coords]
        else:
            chunk = amulet.api.chunk.Chunk(cx, cz)
            self.chunk_cache[coords] = chunk
            self.world.put_chunk(chunk, 'minecraft:overworld')

            for x in range(16):
                for z in range(16):
                    chunk.set_block(x, 0, z, self.BEDROCK)

            chunk.changed = True

            return chunk

    def cleanup(self):
        for chunk in self.chunk_cache.values():
            chunk.changed = True
            self.world.put_chunk(chunk, 'minecraft:overworld')

    def set_block(self, x, y, z, block):
        (cx, lx) = divmod(x, 16)
        (cz, lz) = divmod(z, 16)

        chunk = self.get_chunk(cx, cz)

        chunk.set_block(lx, y, lz, block)

    def add_tile_entity(self, entity):
        pos = (
            entity.x,
            entity.y,
            entity.z,
        )
        cx = pos[0] // 16
        cz = pos[2] // 16

        chunk = self.get_chunk(cx, cz)
        chunk.block_entities.insert(entity)

    def save(self):
        self.cleanup()
        self.world.save()

def main():
    config = parse_args()

    with open(config.INFILE) as inf:
        in_data = json.load(inf)

    world = amulet.level.load_level(config.OUTPUT_WORLD)
    world = WorldWrapper(world)

    def json_to_nbt(data):
        properties = {}
        for k, v in data.items():
            if isinstance(v, dict):
                properties[k] = amulet_nbt.TAG_Compound(json_to_nbt(v))
            elif isinstance(v, str):
                properties[k] = amulet_nbt.TAG_String(v)
            elif isinstance(v, bool):
                if v:
                    properties[k] = amulet_nbt.TAG_String("true")
                else:
                    properties[k] = amulet_nbt.TAG_String("false")
            else:
                properties[k] = amulet_nbt.TAG_Byte(v)
        return properties

    def json_to_block(data):
        properties = None
        if 'properties' in data:
            properties = json_to_nbt(data['properties'])
        fqn = data['name']
        i = fqn.find(':')
        namespace = fqn[:i]
        base_name = fqn[i+1:]
        return amulet.Block(namespace, base_name, properties)

    palette = list(map(json_to_block, in_data['palette']))
    block_data = in_data['blocks']

    ex = in_data['extents']['x']
    ey = in_data['extents']['y']
    ez = in_data['extents']['z']

    for y in range(ey):
        for x in range(ex):
            for z in range(ez):
                i = x + z * ex + y * ex * ez
                block_info = block_data[i]
                block = palette[block_info['pi']]

                xx = x + config.base_x
                yy = y + config.base_y
                zz = z + config.base_z

                world.set_block(xx, yy, zz, block)

                if 'nbt' in block_info:
                    entity_data = json_to_nbt(block_info['nbt'])
                    entity_data = amulet_nbt.NBTFile(amulet_nbt.TAG_Compound(entity_data))
                    entity_data['x'] = amulet_nbt.TAG_Int(xx)
                    entity_data['y'] = amulet_nbt.TAG_Int(yy)
                    entity_data['z'] = amulet_nbt.TAG_Int(zz)
                    entity_data['keepPacked'] = amulet_nbt.TAG_Int(0)

                    entity = amulet.api.block_entity.BlockEntity(
                        block_info['namespace'],
                        block_info['base_name'],
                        xx, yy, zz,
                        entity_data
                    )

                    world.add_tile_entity(entity)


    world.save()

if __name__ == '__main__':
    main()
