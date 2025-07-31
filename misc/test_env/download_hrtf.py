import os
import requests
from urllib.parse import urlparse

def download_file(url, save_path):
    """
    Downloads a file from a given URL and saves it to the same directory as the script.

    Args:
        url (str): The URL of the file to download.
    """
    try:
        # Get the directory where the script is located. [2, 4]
        # __file__ is a special variable that contains the path to the current script.


        # Send a GET request to the URL. [5, 7]
        # stream=True allows downloading large files without consuming too much memory. [1, 5, 10]
        response = requests.get(url, stream=True)
        response.raise_for_status()  # Raise an exception for bad status codes (4xx or 5xx)

        # Extract the filename from the URL. [9, 13]
        parsed_url = urlparse(url)
        filename = os.path.basename(parsed_url.path)

        # If the filename is empty (e.g., for URLs like 'http://example.com/'),
        # try to get it from the Content-Disposition header.
        if not filename:
            if 'content-disposition' in response.headers:
                header = response.headers['content-disposition']
                filename = header.split('filename=')[1].strip('"\'')
            else:
                # As a last resort, use a default name
                filename = "downloaded_file"
        
        # Create the full path to save the file. [8, 12]
        # save_path = os.path.join(script_dir, filename)
        print(f"Attempting to save file to: {save_path}")

        # Save the file to the specified path in binary write mode. [1, 5, 15]
        with open(save_path, 'wb') as f:
            # Download the file in chunks to handle large files efficiently. [1, 10]
            for chunk in response.iter_content(chunk_size=8192):
                if chunk:  # filter out keep-alive new chunks
                    f.write(chunk)
        
        print(f"\nSuccess! File downloaded to: {save_path}")

    except requests.exceptions.RequestException as e:
        print(f"Error downloading the file: {e}")
    except IOError as e:
        print(f"Error saving the file: {e}")
    except Exception as e:
        print(f"An unexpected error occurred: {e}")

if __name__ == "__main__":
    script_dir = os.path.dirname(os.path.abspath(__file__))
    print(f"Script is located in: {script_dir}")
    
    file_url = "https://sofacoustics.org/data/database/ari%20(artificial)/hrtf%20b_nh172.sofa"
    download_file(file_url, os.path.join(script_dir, "../../assets/hrtf/hrtf_b_nh172.sofa"))
