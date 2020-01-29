import topology
from PIL import Image
import numpy
import networkx as nx

im = Image.open('okla_dem_test.tif')
imarray = numpy.array(im)

imgraph = nx.Graph()

# shape is (rows, cols), (y, x)

height, width = imarray.shape

valueMap = []

#indexCount = 0

for x in range(width):
    for y in range(height):
        idx = x + width * y
        valueMap.append((idx, imarray[y, x]))

        # if we're not on the leastmost column,
        if(x > 0):
            # add edge of column left/less
            neighbor_idx = (x - 1) + width * y
            imgraph.add_edge(idx, neighbor_idx)
        # if we're not on the maximum column,
        if(x < (width - 1)):
            # add edge of column right/greater
            neighbor_idx = (x + 1) + width * y
            imgraph.add_edge(idx, neighbor_idx)
        # if we're not on the leastmost row,
        if(y > 0):
            # add edge of row above/less
            neighbor_idx = x + width * (y - 1)
            imgraph.add_edge(idx, neighbor_idx)
        # if we're not on the maxmimum row,
        if(y < (height - 1)):
            # add edge of row below/greater
            neighbor_idx = x + width * (y + 1)
            imgraph.add_edge(idx, neighbor_idx)

# Now we can use topology.
# topology.persistence(Nodes, edges)
# “Nodes” is an (int, float) list of (node Id, scalar value) pairs,
# “edges” is an (int, int) list of edges between node ids.

# valueMap is our list of Nodes and values.
# Edges we need to extract from our nx graph.

edgeList = list(imgraph.edges(data=False))

result = topology.persistence(valueMap, edgeList)

breakpoint()
