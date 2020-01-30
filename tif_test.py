import talus.morse as morse
from PIL import Image
import numpy
import networkx as nx
import cProfile

tif_filename = 'pikes_dem.tif'

im = Image.open(tif_filename)
imarray = numpy.array(im)

imgraph = nx.Graph()

# shape is (rows, cols), (y, x)

height, width = imarray.shape

nodes = []


#FIXME: A lot of this is redundant and can be culled...
# eg, networkx is VERY good about intelligently growing:
#   "If some edges connect nodes not yet in the graph,
#    the nodes are added automatically. There are no
#    errors when adding nodes or edges that already exist."
#
# Also, if the boundaries were treated separately you could
# probably skip ever other row

for x in range(width):
    for y in range(height):
        idx = x + width * y
        value = imarray[y, x]
        my_node = morse.MorseNode(identifier=idx, value=value)
        nodes.append(my_node)

        # if we're not on the leastmost column,
        if(x > 0):
            # add edge of column left/less
            neighbor_idx = (x - 1) + width * y
            neighbor_value = imarray[y, (x - 1)]
            neighbor_node = morse.MorseNode(identifier=neighbor_idx, value=neighbor_value)
            imgraph.add_edge(my_node, neighbor_node)
        # if we're not on the maximum column,
        if(x < (width - 1)):
            # add edge of column right/greater
            neighbor_idx = (x + 1) + width * y
            neighbor_value = imarray[y, (x + 1)]
            neighbor_node = morse.MorseNode(identifier=neighbor_idx, value=neighbor_value)
            imgraph.add_edge(my_node, neighbor_node)
        # if we're not on the leastmost row,
        if(y > 0):
            # add edge of row above/less
            neighbor_idx = x + width * (y - 1)
            neighbor_value = imarray[(y - 1), x]
            neighbor_node = morse.MorseNode(identifier=neighbor_idx, value=neighbor_value)
            imgraph.add_edge(my_node, neighbor_node)
        # if we're not on the maxmimum row,
        if(y < (height - 1)):
            # add edge of row below/greater
            neighbor_idx = x + width * (y + 1)
            neighbor_value = imarray[(y + 1), x]
            neighbor_node = morse.MorseNode(identifier=neighbor_idx, value=neighbor_value)
            imgraph.add_edge(my_node, neighbor_node)

# Now we can use the topology function.

cProfile.run('result = morse.persistence(imgraph)')

for x in range(width):
    for y in range(height):
        idx = x + width * y
        if result[idx] == float('inf'):
            imarray[y, x] = 9999
        else:
            imarray[y, x] = int(result[idx] * 100)



# Esri ASCII Raster Format
# http://resources.esri.com/help/9.3/ArcGISengine/java/Gp_ToolRef/Spatial_Analyst_Tools/esri_ascii_raster_format.htm

outF = open((tif_filename + ".asc"), "w")

outF.write("ncols " + str(width))
outF.write("\n")

outF.write("nrows " + str(height))
outF.write("\n")

# -98.5684,34.7588
# xllcorner -985684
# yllcorner 347588
outF.write("xllcorner -105.1260185189999987")
outF.write("\n")
outF.write("yllcorner 38.8015740740000012")
outF.write("\n")
outF.write("cellsize 0.00009259259259259258799")
outF.write("\n")

for y in range(height):
    for x in range(width):
        outF.write(str(imarray[y, x]))
        outF.write(" ")
    outF.write("\n")

outF.close()
