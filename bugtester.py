import networkx as nx
import pickle
import talus.morse as morse

filename = 'test_nxgraph.pickle'

pickleF = open((filename), 'rb')

imgraph = pickle.load(pickleF)

print("running persistence")
result = morse.persistence(imgraph)

print(result.descending_complex.compute_cells_at_lifetime(0))
for f in result.descending_complex.filtration:

    print(result.descending_complex.compute_cells_at_lifetime(f.lifetime))



# for f in result.descending_complex.filtration:
#     print(f)
